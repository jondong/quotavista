//! Lifecycle orchestration: per-connection mutation fences, external journals,
//! and recovery before scheduling. Fixtures are synthetic; the value-carrying
//! stores behind [`SecretReferenceStore`] live in `crate::secrets`.
//!
//! The coordinator owns the external safety state and is the only writer, so
//! `&mut self` *is* the mutation fence: while a caller holds the borrow no other
//! lifecycle mutation can interleave, and every operation holds it through its
//! commit. The database re-checks the generation and the fence inside the write
//! transaction as the final protection, so a stale result cannot commit even if a
//! caller ignored the fence.
//!
//! Ordering rule used everywhere below: write the external intent durably first,
//! then touch a secret, then the database projection, and release the fence last.
//! An interruption therefore always leaves recoverable state behind, never a
//! silent success.

use quotavista_core::{
    domain::Lifecycle,
    journal::{
        reconcile, BackupManifest, DurableStore, Journal, JournalError, Ledger, LedgerState,
        OperationKind, OperationPhase, OperationRecord, RecoveryAction, SecretReferenceStore,
        SecretStoreError,
    },
    storage::{Repository, WriteContext},
};

/// Entry name of the external lifecycle journal.
pub const JOURNAL_NAME: &str = "lifecycle-journal.v1";
/// Entry name of the external deletion ledger.
pub const LEDGER_NAME: &str = "deletion-ledger.v1";

/// What one recovery run finished and what remains visibly pending.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    /// Actions whose owed work completed.
    pub finished: Vec<RecoveryAction>,
    /// Actions that could not complete, for example because a secret store is
    /// locked. They stay visible and retryable, and nothing is acknowledged.
    pub pending: Vec<RecoveryAction>,
}

/// Serializes lifecycle mutations and keeps the external journals, their database
/// projections, and the deletion ledger consistent.
pub struct Coordinator<R, D, S> {
    repository: R,
    durable: D,
    secrets: S,
    journal: Journal,
    ledger: Ledger,
    recovered: bool,
}

impl<R: Repository, D: DurableStore, S: SecretReferenceStore + Clone + Send + 'static>
    Coordinator<R, D, S>
{
    /// Load the external safety state. Corrupt or unreadable state is a visible
    /// recovery condition: no coordinator is returned, so nothing can be
    /// scheduled or mutated from it.
    pub fn open(repository: R, durable: D, secrets: S) -> Result<Self, JournalError> {
        let journal = match durable.read(JOURNAL_NAME)? {
            Some(text) => Journal::decode(&text)?,
            None => Journal::default(),
        };
        let ledger = match durable.read(LEDGER_NAME)? {
            Some(text) => Ledger::decode(&text)?,
            None => Ledger::default(),
        };
        Ok(Self {
            repository,
            durable,
            secrets,
            journal,
            ledger,
            recovered: false,
        })
    }

    /// External journal state as loaded and maintained in memory.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Deletion ledger as loaded and maintained in memory.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Reconcile the external journals with the database projections and finish
    /// the owed work. Idempotent: running it again converges to the same state.
    /// Scheduling and lifecycle mutations are refused until it succeeds.
    ///
    /// A secret store that is locked or unavailable leaves the affected actions in
    /// [`RecoveryReport::pending`] instead of failing the run, so the remaining
    /// connections can still be scheduled while their fences stay up.
    pub async fn recover(&mut self) -> Result<RecoveryReport, JournalError> {
        let projections = self
            .repository
            .operations(None)
            .await
            .map_err(JournalError::from)?;
        let actions = reconcile(&self.journal, &self.ledger, &projections);
        let mut report = RecoveryReport::default();
        let mut blocked: Vec<String> = Vec::new();
        for action in actions {
            let operation = action_operation(&action);
            if operation
                .as_deref()
                .is_some_and(|id| blocked.iter().any(|blocked| blocked == id))
            {
                // The operation already owes work recovery could not finish.
                report.pending.push(action);
                continue;
            }
            let outcome = match &action {
                RecoveryAction::DeleteReference { reference, .. } => self
                    .store_call({
                        let reference = reference.clone();
                        move |store| store.delete_reference(&reference)
                    })
                    .await
                    .map_err(JournalError::from),
                RecoveryAction::FinishOperation { record } => self.finish(record.clone()).await,
                RecoveryAction::DeletionPending { opaque_id } => {
                    self.resume_deletion(opaque_id).await
                }
            };
            match outcome {
                Ok(()) => report.finished.push(action),
                // A locked store, an unavailable boundary, or an unacknowledged
                // deletion leaves the action visibly pending, and the rest of that
                // operation is skipped with it.
                Err(
                    JournalError::Locked
                    | JournalError::Unavailable
                    | JournalError::RecoveryRequired,
                ) => {
                    if let Some(id) = operation {
                        blocked.push(id);
                    }
                    report.pending.push(action);
                }
                Err(error) => return Err(error),
            }
        }
        // Recovery ran, so the remaining connections may be scheduled. Anything it
        // could not finish stays fenced by the journal and ledger it reloaded.
        self.recovered = true;
        Ok(report)
    }

    /// Admission for scheduling or dispatching one connection. Recovery must have
    /// run, no journal record or ledger entry may fence the connection or any of
    /// its scopes, and the connection must be active. The returned context carries
    /// the current generation, which the write transaction re-checks.
    pub async fn schedule_context(
        &self,
        connection_id: &str,
    ) -> Result<WriteContext, JournalError> {
        self.require_recovered()?;
        self.assert_unfenced(connection_id)?;
        // Database projections fence a connection whose state this coordinator did
        // not write, so admission consults them as well as the external journals.
        if !self
            .repository
            .operations(Some(connection_id))
            .await
            .map_err(JournalError::from)?
            .is_empty()
        {
            return Err(JournalError::Conflict);
        }
        for scope in self
            .repository
            .scope_ids(connection_id)
            .await
            .map_err(JournalError::from)?
        {
            if self.ledger.blocks(&scope) {
                return Err(JournalError::Conflict);
            }
        }
        let connection = self
            .repository
            .connection(connection_id)
            .await
            .map_err(JournalError::from)?
            .ok_or(JournalError::Validation)?;
        if connection.lifecycle != Lifecycle::Active {
            return Err(JournalError::Conflict);
        }
        Ok(WriteContext {
            connection_id: connection.id,
            generation: connection.generation,
        })
    }

    /// Durable setup or credential replacement: journal the intent, write the
    /// candidate, then activate it in one write transaction that re-checks the
    /// generation and the fence. A failure compensates the candidate instead of
    /// leaving an orphan. Returns the generation after activation.
    pub async fn activate_credentials(
        &mut self,
        operation_id: &str,
        context: &WriteContext,
        kind: OperationKind,
        candidate_ref: &str,
    ) -> Result<i64, JournalError> {
        self.require_recovered()?;
        if kind == OperationKind::Delete {
            return Err(JournalError::Validation);
        }
        let connection_id = context.connection_id.clone();
        self.assert_unfenced(&connection_id)?;
        // The reference a replacement supersedes is read inside the fence, so a
        // concurrent mutation cannot make the caller's value stale. An operation
        // identifier is never reused, so a retry after a crash cannot resurrect
        // state from the attempt that failed.
        let previous_ref = self
            .repository
            .credential_ref(&connection_id)
            .await
            .map_err(JournalError::from)?;
        // A reference the connection already holds has nothing to supersede and
        // nothing to compensate, so it is refused before any state is written. That
        // keeps every later step honest: neither `abort` nor a projection that
        // recovery resumes can end up owing cleanup of a secret still in use.
        if previous_ref.as_deref() == Some(candidate_ref) {
            return Err(JournalError::Validation);
        }
        let record = OperationRecord {
            operation_id: operation_id.to_owned(),
            connection_id,
            kind,
            phase: OperationPhase::Pending,
            candidate_ref: Some(candidate_ref.to_owned()),
            previous_ref,
            generation: context.generation,
        };
        record.validate()?;

        // 1. The intent is durable before any secret write or database change, so
        //    a failure here stops the mutation with nothing to compensate.
        self.commit_journal(|journal| journal.record(record.clone()))?;
        // 2. The projection fences result writes for this connection.
        if let Err(error) = self.repository.begin_operation(&record).await {
            return Err(self.abort(record, error.into()).await);
        }
        // 3. Record the boundary the candidate write happens at.
        let mut record = self.step(&record, OperationPhase::CandidateWritten)?;
        let candidate = candidate_ref.to_owned();
        if let Err(error) = self
            .store_call(move |store| store.write_candidate(&candidate))
            .await
        {
            return Err(self.abort(record, error.into()).await);
        }
        // 4. Activation: the final generation and fence check runs inside the
        //    write transaction, so an obsolete generation cannot commit.
        let generation = match self
            .repository
            .activate_reference(
                context,
                operation_id,
                candidate_ref,
                record.previous_ref.as_deref(),
            )
            .await
        {
            Ok(generation) => generation,
            Err(error) => return Err(self.abort(record, error.into()).await),
        };
        // 5. The activation transaction already committed the projection, so make
        //    the external journal agree before the superseded reference is
        //    removed: a crash here cannot forget the cleanup that is owed.
        record = self.step(&record, OperationPhase::Committed)?;
        // 6. Remove the superseded reference. A failure leaves the operation
        //    visibly pending cleanup instead of acknowledging the replacement;
        //    recovery resumes it, and the fence stays up meanwhile.
        match self.owe_cleanup(&record).await {
            Ok(()) => {}
            Err(error) => return Err(self.mark_compensation(record, error.into()).await),
        }
        // 7. Release the fence last.
        self.finish(record).await?;
        Ok(generation)
    }

    /// Delete the reference this operation owes, if any: the candidate while it is
    /// still uncommitted, the superseded one afterwards.
    async fn owe_cleanup(&self, record: &OperationRecord) -> Result<(), SecretStoreError> {
        let owed = if record.phase.committed() {
            record.previous_ref.clone()
        } else {
            record.candidate_ref.clone()
        };
        match owed {
            Some(reference) => {
                self.store_call(move |store| store.delete_reference(&reference))
                    .await
            }
            None => Ok(()),
        }
    }

    /// Run one blocking store call off the async worker: a stalled keychain
    /// prompt or D-Bus round-trip must not occupy a tokio worker. The
    /// coordinator runs on a tokio runtime. A panicked or cancelled store task
    /// leaves the backend state unknown, so it reports unavailable, never
    /// locked.
    async fn store_call<T>(
        &self,
        call: impl FnOnce(&S) -> Result<T, SecretStoreError> + Send + 'static,
    ) -> Result<T, SecretStoreError>
    where
        T: Send + 'static,
    {
        let store = self.secrets.clone();
        match tokio::task::spawn_blocking(move || call(&store)).await {
            Ok(result) => result,
            Err(_) => Err(SecretStoreError::Unavailable),
        }
    }

    /// Begin a deletion: the ledger is durable before the lifecycle transition, so
    /// a restored database cannot resurrect scheduling for these identifiers.
    /// Returns the generation after the transition. Cleanup is owed until
    /// [`Coordinator::finish_deletion`] succeeds.
    pub async fn begin_deletion(
        &mut self,
        operation_id: &str,
        context: &WriteContext,
    ) -> Result<i64, JournalError> {
        self.require_recovered()?;
        let connection_id = context.connection_id.clone();
        // A journal record means this connection already owes a deletion: finish
        // that one instead of starting a second. A ledger entry without an
        // operation is an interrupted deletion, which this call resumes, so it must
        // not be refused here.
        if self.journal.fences(&connection_id) {
            return Err(JournalError::Conflict);
        }
        let credential_ref = self
            .repository
            .credential_ref(&connection_id)
            .await
            .map_err(JournalError::from)?;
        let scopes = self
            .repository
            .scope_ids(&connection_id)
            .await
            .map_err(JournalError::from)?;
        // 1. Fence every opaque identifier the deletion owns.
        self.commit_ledger(|ledger| {
            ledger.insert_pending(&connection_id, None)?;
            for scope in &scopes {
                ledger.insert_pending(scope, Some(&connection_id))?;
            }
            Ok(())
        })?;
        // 2. Durable transition with a generation increment, which invalidates any
        //    context a running task captured.
        let generation = self
            .repository
            .set_lifecycle(context, Lifecycle::DeletionPending)
            .await
            .map_err(JournalError::from)?;
        // 3. Journal the owed cleanup so it resumes after a restart. For a
        //    deletion the candidate field carries the reference to remove. If this
        //    write is not durable the ledger fence and the transition stay in place
        //    and the retry re-reads the generation, so a reference is never removed
        //    before the intent to remove it is durable.
        let record = OperationRecord {
            operation_id: operation_id.to_owned(),
            connection_id,
            kind: OperationKind::Delete,
            phase: OperationPhase::Pending,
            candidate_ref: credential_ref,
            previous_ref: None,
            generation,
        };
        record.validate()?;
        self.commit_journal(|journal| journal.record(record.clone()))?;
        self.repository
            .begin_operation(&record)
            .await
            .map_err(JournalError::from)?;
        Ok(generation)
    }

    /// Finish a deletion's cleanup: remove the owed secret reference, acknowledge
    /// the ledger, then release the fence. A failure leaves the deletion pending
    /// and unacknowledged, so it stays visible and retryable across restarts.
    pub async fn finish_deletion(&mut self, operation_id: &str) -> Result<(), JournalError> {
        self.require_recovered()?;
        let record = self.operation(operation_id).await?;
        if record.kind != OperationKind::Delete {
            return Err(JournalError::Validation);
        }
        if let Some(reference) = record.candidate_ref.clone() {
            self.store_call(move |store| store.delete_reference(&reference))
                .await
                .map_err(JournalError::from)?;
        }
        self.finish(record).await
    }

    /// Restore admission for a managed backup manifest. A deletion that still owes
    /// cleanup blocks any restore, and a manifest listing an identifier the ledger
    /// fences is rejected so an acknowledged deletion cannot return.
    pub fn admit_restore(&self, manifest: &BackupManifest) -> Result<(), JournalError> {
        self.require_recovered()?;
        manifest.validate()?;
        if self.ledger.has_pending_cleanup() {
            return Err(JournalError::RecoveryRequired);
        }
        for id in &manifest.included_ids {
            if self.ledger.blocks(id) {
                return Err(JournalError::Conflict);
            }
        }
        Ok(())
    }

    /// Current record for an operation. The external journal is authoritative; the
    /// database projection covers a journal that was never written.
    async fn operation(&self, operation_id: &str) -> Result<OperationRecord, JournalError> {
        if let Some(record) = self.journal.get(operation_id) {
            return Ok(record.clone());
        }
        self.repository
            .operations(None)
            .await
            .map_err(JournalError::from)?
            .into_iter()
            .find(|record| record.operation_id == operation_id)
            .ok_or(JournalError::Validation)
    }

    /// Release one operation's fence: acknowledge a deletion's ledger entries,
    /// drop the external record, then complete the projection last so an
    /// interruption always leaves recoverable state behind.
    async fn finish(&mut self, mut record: OperationRecord) -> Result<(), JournalError> {
        if record.kind == OperationKind::Delete {
            self.acknowledge_deletion(&record.connection_id)?;
        }
        let operation_id = record.operation_id.clone();
        match self
            .commit_journal(move |journal| journal.advance(&operation_id, OperationPhase::Complete))
        {
            // The external journal never held this operation, so only the
            // projection owes completion.
            Ok(_) | Err(JournalError::Validation) => {}
            Err(error) => return Err(error),
        }
        record.phase = OperationPhase::Complete;
        self.repository
            .advance_operation(&record)
            .await
            .map_err(JournalError::from)
    }

    /// Mark every ledger entry a deletion owns as acknowledged. Called only after
    /// every cleanup step this build owns succeeded.
    fn acknowledge_deletion(&mut self, connection_id: &str) -> Result<(), JournalError> {
        let mut owned = self.ledger.owned_by(connection_id);
        if self.ledger.blocks(connection_id) {
            owned.push(connection_id.to_owned());
        }
        self.commit_ledger(move |ledger| {
            for id in &owned {
                if ledger.state(id) == Some(LedgerState::PendingCleanup) {
                    ledger.mark_cleanup_complete(id)?;
                }
            }
            Ok(())
        })
    }

    /// Resume a deletion the ledger fenced but no operation recorded. An
    /// interruption after the committed `deletion_pending` transition and before
    /// the journal write leaves nothing to finish, so the ledger supplies the
    /// connection and the database the reference it owes. Anything that blocks the
    /// cleanup, including a transition that never committed, leaves the entry
    /// pending: visible and retryable, never reported clean.
    async fn resume_deletion(&mut self, opaque_id: &str) -> Result<(), JournalError> {
        // A scope entry is acknowledged with the connection that owns it, so resume
        // through the owner: recovery then converges in one pass whichever order the
        // sorted ledger entries are visited in. The final state check below still
        // names this entry, so an unresolved scope is never reported clean.
        let target = self
            .ledger_owner(opaque_id)
            .unwrap_or_else(|| opaque_id.to_owned());
        if self.resumable_deletion(&target).await? {
            // A missing row leaves the owed reference unknown, so the deletion stays
            // pending instead of being acknowledged.
            let owed = match self.repository.credential_ref(&target).await {
                Ok(owed) => owed,
                Err(_) => return Err(JournalError::RecoveryRequired),
            };
            if let Some(reference) = owed {
                // A locked or unavailable store leaves the cleanup owed.
                self.store_call(move |store| store.delete_reference(&reference))
                    .await
                    .map_err(|_| JournalError::RecoveryRequired)?;
            }
            match self.acknowledge_deletion(&target) {
                // An acknowledgement that is not durable leaves the entry pending,
                // so the next recovery retries it.
                Ok(()) | Err(JournalError::Durability) => {}
                Err(error) => return Err(error),
            }
        }
        if self.ledger.state(opaque_id) == Some(LedgerState::PendingCleanup) {
            Err(JournalError::RecoveryRequired)
        } else {
            Ok(())
        }
    }

    /// Whether a pending ledger entry belongs to a deletion no operation recorded.
    /// Scope entries resume through the connection that owns them; a deletion the
    /// journal or a projection still carries resumes through its own actions. Only a
    /// committed `deletion_pending` transition owes the reference, so a refused
    /// compare-and-swap leaves the entry pending for `begin_deletion` to retry.
    async fn resumable_deletion(&self, opaque_id: &str) -> Result<bool, JournalError> {
        if self.ledger.state(opaque_id) != Some(LedgerState::PendingCleanup)
            || self.journal.fences(opaque_id)
        {
            return Ok(false);
        }
        if !self
            .repository
            .operations(Some(opaque_id))
            .await
            .map_err(JournalError::from)?
            .is_empty()
        {
            return Ok(false);
        }
        // The ledger is written before the transition, so a pending entry alone does
        // not prove the deletion was accepted. Removing the reference a row still
        // points at would strand an active connection without its credential.
        Ok(self
            .repository
            .connection(opaque_id)
            .await
            .map_err(JournalError::from)?
            .is_some_and(|connection| connection.lifecycle == Lifecycle::DeletionPending))
    }

    /// Connection a ledger entry belongs to, when it fences a scope rather than the
    /// connection itself.
    fn ledger_owner(&self, opaque_id: &str) -> Option<String> {
        self.ledger
            .entries()
            .find(|(id, _)| id.as_str() == opaque_id)
            .and_then(|(_, entry)| entry.owner.clone())
    }

    /// Compensate a failed activation: remove the orphan candidate, then release
    /// the fence. When the candidate cannot be removed the operation stays visibly
    /// pending cleanup and recovery retries it. Returns the error to report.
    async fn abort(&mut self, record: OperationRecord, cause: JournalError) -> JournalError {
        let removal = match record.candidate_ref.clone() {
            Some(reference) => self
                .store_call(move |store| store.delete_reference(&reference))
                .await
                .map_err(JournalError::from),
            None => Ok(()),
        };
        if let Err(error) = removal {
            return self.mark_compensation(record, error).await;
        }
        self.finish(record).await.err().unwrap_or(cause)
    }

    /// Record that cleanup is owed and report `cause`. Best effort: when even that
    /// cannot be persisted, recovery still finds the owed reference in the journal
    /// or the projection.
    async fn mark_compensation(
        &mut self,
        record: OperationRecord,
        cause: JournalError,
    ) -> JournalError {
        let pending = OperationRecord {
            phase: OperationPhase::CompensationPending,
            ..record
        };
        self.commit_journal(|journal| journal.record(pending))
            .err()
            .unwrap_or(cause)
    }

    /// Persist one journal phase change before the step it guards happens.
    fn step(
        &mut self,
        record: &OperationRecord,
        phase: OperationPhase,
    ) -> Result<OperationRecord, JournalError> {
        let operation_id = record.operation_id.clone();
        self.commit_journal(move |journal| journal.advance(&operation_id, phase))
    }

    fn assert_unfenced(&self, connection_id: &str) -> Result<(), JournalError> {
        if self.journal.fences(connection_id) || self.ledger.blocks(connection_id) {
            return Err(JournalError::Conflict);
        }
        Ok(())
    }

    fn require_recovered(&self) -> Result<(), JournalError> {
        if self.recovered {
            Ok(())
        } else {
            Err(JournalError::RecoveryRequired)
        }
    }

    /// Apply one journal change and persist it before adopting it in memory, so a
    /// durability failure never leaves memory claiming state the file lacks.
    fn commit_journal<T>(
        &mut self,
        change: impl FnOnce(&mut Journal) -> Result<T, JournalError>,
    ) -> Result<T, JournalError> {
        let mut journal = self.journal.clone();
        let value = change(&mut journal)?;
        self.durable.replace(JOURNAL_NAME, &journal.encode())?;
        self.journal = journal;
        Ok(value)
    }

    /// Ledger twin of [`Coordinator::commit_journal`].
    fn commit_ledger<T>(
        &mut self,
        change: impl FnOnce(&mut Ledger) -> Result<T, JournalError>,
    ) -> Result<T, JournalError> {
        let mut ledger = self.ledger.clone();
        let value = change(&mut ledger)?;
        self.durable.replace(LEDGER_NAME, &ledger.encode())?;
        self.ledger = ledger;
        Ok(value)
    }
}

/// Operation an action belongs to, when it is operation-scoped.
fn action_operation(action: &RecoveryAction) -> Option<String> {
    match action {
        RecoveryAction::DeleteReference { operation_id, .. }
        | RecoveryAction::FinishOperation {
            record: OperationRecord { operation_id, .. },
        } => Some(operation_id.clone()),
        RecoveryAction::DeletionPending { .. } => None,
    }
}

#[cfg(test)]
mod tests;
