//! Crash-safe lifecycle journals, deletion ledgers, and recovery decisions.
//!
//! Every value here is an opaque local identifier or nonsecret operation state.
//! Credential values, provider identity, and usage data never enter a journal, a
//! ledger, or their durable encoding. Journals and ledgers live outside the
//! backed-up database so a managed restore cannot forget a newly created or
//! superseded secret reference, and cannot silently resurrect a deletion that
//! was already acknowledged.

use crate::storage::StorageError;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

/// Encoded absence of an optional opaque reference.
const ABSENT: &str = "-";
/// Longest accepted opaque identifier, in bytes.
const MAX_ID_BYTES: usize = 256;
/// Fields per encoded journal record.
const JOURNAL_FIELDS: usize = 7;
/// Fields per encoded ledger entry.
const LEDGER_FIELDS: usize = 3;

const JOURNAL_HEADER: &str = "quotavista lifecycle journal v1";
const LEDGER_HEADER: &str = "quotavista deletion ledger v1";

/// Why durable lifecycle state could not be established, read, or used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalError {
    /// A record, identifier, or manifest failed validation.
    Validation,
    /// Durable state could not be read back as written. Corruption is a visible
    /// recovery condition: absent or unreadable state never means that cleanup
    /// already finished, so recovery refuses to schedule.
    Corrupt,
    /// Durability could not be established. The lifecycle mutation must stop
    /// before a secret is written or a deletion is acknowledged.
    Durability,
    /// Recovery has not reconciled the journals yet, so scheduling and lifecycle
    /// mutations are refused.
    RecoveryRequired,
    /// A fence, generation, revision, or admission check rejected the request.
    Conflict,
    /// The secret store is locked. The operation stays visibly pending and must
    /// never be reported as successful.
    Locked,
    /// A storage or filesystem dependency was unavailable.
    Unavailable,
}

/// Nonsecret kind of a journaled lifecycle operation. The wire vocabulary in
/// `quotavista-contracts` maps onto these values; the journal itself never
/// carries provider identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    /// First credential for a new connection.
    Setup,
    /// Rotation that keeps the previous credential active until it commits.
    ReplaceCredentials,
    /// Connection removal.
    Delete,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::ReplaceCredentials => "replace_credentials",
            Self::Delete => "delete",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "setup" => Some(Self::Setup),
            "replace_credentials" => Some(Self::ReplaceCredentials),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Durable progress of one journaled operation. Any phase other than
/// [`OperationPhase::Complete`] fences result writes and dispatch for the
/// connection, including across restarts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPhase {
    /// Intent is durable; no secret has been written and nothing committed.
    Pending,
    /// A candidate secret reference exists but the database has not activated it.
    CandidateWritten,
    /// The database committed the mutation; a superseded reference may still owe
    /// cleanup.
    Committed,
    /// Activation failed after the candidate was written, so the candidate must
    /// be removed. A failed compensation stays visibly pending.
    CompensationPending,
    /// Every step finished; the record no longer fences the connection.
    Complete,
}

impl OperationPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::CandidateWritten => "candidate_written",
            Self::Committed => "committed",
            Self::CompensationPending => "compensation_pending",
            Self::Complete => "complete",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "candidate_written" => Some(Self::CandidateWritten),
            "committed" => Some(Self::Committed),
            "compensation_pending" => Some(Self::CompensationPending),
            "complete" => Some(Self::Complete),
            _ => None,
        }
    }

    /// Whether this phase still fences the connection.
    pub fn fences(self) -> bool {
        self != Self::Complete
    }

    /// Whether the database committed the mutation, so the candidate reference
    /// is now authoritative and must not be deleted.
    pub fn committed(self) -> bool {
        matches!(self, Self::Committed | Self::Complete)
    }

    /// Legal forward transitions. Recovery may abort an uncommitted operation or
    /// finish a committed one once every owed reference is gone, so `Complete` is
    /// reachable from every earlier phase.
    fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::CandidateWritten)
                | (Self::Pending, Self::Committed)
                | (Self::Pending, Self::CompensationPending)
                | (Self::Pending, Self::Complete)
                | (Self::CandidateWritten, Self::Committed)
                | (Self::CandidateWritten, Self::CompensationPending)
                | (Self::CandidateWritten, Self::Complete)
                | (Self::CompensationPending, Self::Complete)
                | (Self::Committed, Self::Complete)
                | (Self::Committed, Self::CompensationPending)
        )
    }
}

/// One journaled lifecycle operation: opaque identifiers and nonsecret state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationRecord {
    pub operation_id: String,
    pub connection_id: String,
    pub kind: OperationKind,
    pub phase: OperationPhase,
    /// Reference created by this operation. It is owed deletion only while the
    /// operation is uncommitted; after a commit it is the active reference.
    pub candidate_ref: Option<String>,
    /// Reference superseded by this operation, owed deletion after a commit.
    pub previous_ref: Option<String>,
    /// Connection generation captured when the intent was journaled.
    pub generation: i64,
}

/// Rejects empty, oversized, control-character, and sentinel-ambiguous values so
/// the durable encoding stays injective and free of free-form text.
fn opaque(value: &str) -> Result<(), JournalError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value == ABSENT
        || value.chars().any(char::is_control)
    {
        return Err(JournalError::Validation);
    }
    Ok(())
}

fn optional(value: &str) -> Result<Option<String>, JournalError> {
    if value == ABSENT {
        return Ok(None);
    }
    opaque(value)?;
    Ok(Some(value.to_owned()))
}

impl OperationRecord {
    pub fn validate(&self) -> Result<(), JournalError> {
        opaque(&self.operation_id)?;
        opaque(&self.connection_id)?;
        if self.generation < 0 {
            return Err(JournalError::Validation);
        }
        for value in [&self.candidate_ref, &self.previous_ref]
            .into_iter()
            .flatten()
        {
            opaque(value)?;
        }
        // A rotation without a candidate has nothing to activate.
        if self.kind != OperationKind::Delete && self.candidate_ref.is_none() {
            return Err(JournalError::Validation);
        }
        Ok(())
    }

    fn encode_into(&self, text: &mut String) {
        text.push_str(&self.operation_id);
        text.push('\t');
        text.push_str(&self.connection_id);
        text.push('\t');
        text.push_str(self.kind.as_str());
        text.push('\t');
        text.push_str(self.phase.as_str());
        text.push('\t');
        text.push_str(self.candidate_ref.as_deref().unwrap_or(ABSENT));
        text.push('\t');
        text.push_str(self.previous_ref.as_deref().unwrap_or(ABSENT));
        text.push('\t');
        text.push_str(&self.generation.to_string());
        text.push('\n');
    }

    fn decode(fields: &[&str]) -> Result<Self, JournalError> {
        if fields.len() != JOURNAL_FIELDS {
            return Err(JournalError::Corrupt);
        }
        let record = Self {
            operation_id: fields[0].to_owned(),
            connection_id: fields[1].to_owned(),
            kind: OperationKind::parse(fields[2]).ok_or(JournalError::Corrupt)?,
            phase: OperationPhase::parse(fields[3]).ok_or(JournalError::Corrupt)?,
            candidate_ref: optional(fields[4])?,
            previous_ref: optional(fields[5])?,
            generation: fields[6]
                .parse::<i64>()
                .map_err(|_| JournalError::Corrupt)?,
        };
        // Validation failures in durable state are corruption, not a caller bug.
        record.validate().map_err(|_| JournalError::Corrupt)?;
        Ok(record)
    }
}

/// The durable, nonsecret journal of secret-reference lifecycle operations.
///
/// The whole journal is replaced atomically on every change; it holds only the
/// handful of operations that are currently in flight or owe cleanup.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Journal {
    records: Vec<OperationRecord>,
}

impl Journal {
    pub fn records(&self) -> &[OperationRecord] {
        &self.records
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, operation_id: &str) -> Option<&OperationRecord> {
        self.records.iter().find(|r| r.operation_id == operation_id)
    }

    /// Whether an external credential journal still fences this connection.
    pub fn fences(&self, connection_id: &str) -> bool {
        self.records
            .iter()
            .any(|r| r.connection_id == connection_id && r.phase.fences())
    }

    /// Connections fenced by an incomplete journal record, sorted and deduplicated.
    pub fn fenced_connections(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .records
            .iter()
            .filter(|r| r.phase.fences())
            .map(|r| r.connection_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Insert or replace a record. Replacing a completed operation is rejected so
    /// a stale retry cannot reopen a finished mutation, and an identifier already
    /// held for another connection is rejected so the journal stays decodable.
    pub fn record(&mut self, record: OperationRecord) -> Result<(), JournalError> {
        record.validate()?;
        if record.phase == OperationPhase::Complete {
            // Completion removes a record, so only `advance` may finish one: the
            // journal never carries an operation whose fence is already released.
            return Err(JournalError::Validation);
        }
        if self.records.iter().any(|r| {
            r.operation_id == record.operation_id && r.connection_id != record.connection_id
        }) {
            // Decoding rejects a repeated operation identifier, so persisting one
            // would leave external state no coordinator could ever read again.
            return Err(JournalError::Conflict);
        }
        match self
            .records
            .iter_mut()
            .find(|r| r.operation_id == record.operation_id)
        {
            Some(existing) => {
                if !existing.phase.allows(record.phase) {
                    return Err(JournalError::Conflict);
                }
                *existing = record;
            }
            None => self.records.push(record),
        }
        Ok(())
    }

    /// Advance one operation, removing it when it completes.
    pub fn advance(
        &mut self,
        operation_id: &str,
        phase: OperationPhase,
    ) -> Result<OperationRecord, JournalError> {
        let index = self
            .records
            .iter()
            .position(|r| r.operation_id == operation_id)
            .ok_or(JournalError::Validation)?;
        let mut record = self.records[index].clone();
        if !record.phase.allows(phase) {
            return Err(JournalError::Conflict);
        }
        record.phase = phase;
        if phase == OperationPhase::Complete {
            self.records.remove(index);
        } else {
            self.records[index] = record.clone();
        }
        Ok(record)
    }

    pub fn encode(&self) -> String {
        let mut text = String::from(JOURNAL_HEADER);
        text.push('\n');
        for record in &self.records {
            record.encode_into(&mut text);
        }
        text
    }

    pub fn decode(text: &str) -> Result<Self, JournalError> {
        let mut lines = text.lines();
        if lines.next() != Some(JOURNAL_HEADER) {
            return Err(JournalError::Corrupt);
        }
        let mut journal = Self::default();
        for line in lines {
            let record = OperationRecord::decode(&line.split('\t').collect::<Vec<_>>())?;
            if journal.get(&record.operation_id).is_some() {
                return Err(JournalError::Corrupt);
            }
            journal.records.push(record);
        }
        Ok(journal)
    }
}

/// Cleanup state of one opaque local identifier in the deletion ledger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedgerState {
    /// Cleanup still owes work, so the deletion is not acknowledged.
    PendingCleanup,
    /// Every cleanup step this build owns finished. The entry is retained until
    /// no managed backup can reintroduce the identifier.
    CleanupComplete,
}

impl LedgerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingCleanup => "pending_cleanup",
            Self::CleanupComplete => "cleanup_complete",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending_cleanup" => Some(Self::PendingCleanup),
            "cleanup_complete" => Some(Self::CleanupComplete),
            _ => None,
        }
    }
}

/// Minimal deletion ledger kept outside the backed-up database. It records only
/// opaque local identifiers, the connection that owns each one, and pending
/// cleanup: no provider identifiers, secrets, or usage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ledger {
    entries: BTreeMap<String, LedgerEntry>,
}

/// One fenced identifier. `owner` is the opaque connection identifier the
/// deletion belongs to, so cleanup can be finished after a restart without
/// re-deriving membership from rows the deletion may already have removed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerEntry {
    /// Owning connection identifier, or [`None`] for the connection's own entry.
    pub owner: Option<String>,
    /// Cleanup state.
    pub state: LedgerState,
}

impl Ledger {
    pub fn entries(&self) -> impl Iterator<Item = (&String, &LedgerEntry)> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn state(&self, opaque_id: &str) -> Option<LedgerState> {
        self.entries.get(opaque_id).map(|entry| entry.state)
    }

    /// Whether the ledger fences an opaque identifier. A retained entry blocks
    /// dispatch and restore admission even after cleanup completed, because a
    /// managed backup could still reintroduce it.
    pub fn blocks(&self, opaque_id: &str) -> bool {
        self.entries.contains_key(opaque_id)
    }

    /// Identifiers fenced by one connection's deletion, in sorted order.
    pub fn owned_by(&self, owner: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.owner.as_deref() == Some(owner))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Whether any deletion still owes cleanup, which also forbids creating a new
    /// managed backup and blocks restore admission.
    pub fn has_pending_cleanup(&self) -> bool {
        self.entries
            .values()
            .any(|entry| entry.state == LedgerState::PendingCleanup)
    }

    /// Record a deletion intent. Re-recording an identifier keeps its state so a
    /// retry cannot downgrade an acknowledged deletion to pending cleanup.
    pub fn insert_pending(
        &mut self,
        opaque_id: &str,
        owner: Option<&str>,
    ) -> Result<(), JournalError> {
        opaque(opaque_id)?;
        if let Some(owner) = owner {
            opaque(owner)?;
        }
        self.entries
            .entry(opaque_id.to_owned())
            .or_insert(LedgerEntry {
                owner: owner.map(str::to_owned),
                state: LedgerState::PendingCleanup,
            });
        Ok(())
    }

    pub fn mark_cleanup_complete(&mut self, opaque_id: &str) -> Result<(), JournalError> {
        match self.entries.get_mut(opaque_id) {
            Some(entry) => {
                entry.state = LedgerState::CleanupComplete;
                Ok(())
            }
            None => Err(JournalError::Validation),
        }
    }

    /// Drop acknowledged identifiers that no managed backup can reintroduce.
    /// Entries still owing cleanup are never compacted. Returns how many were
    /// removed.
    pub fn compact(&mut self, reintroducible: &[String]) -> usize {
        let before = self.entries.len();
        self.entries.retain(|id, entry| {
            entry.state == LedgerState::PendingCleanup || reintroducible.contains(id)
        });
        before - self.entries.len()
    }

    pub fn encode(&self) -> String {
        let mut text = String::from(LEDGER_HEADER);
        text.push('\n');
        for (id, entry) in &self.entries {
            text.push_str(id);
            text.push('\t');
            text.push_str(entry.owner.as_deref().unwrap_or(ABSENT));
            text.push('\t');
            text.push_str(entry.state.as_str());
            text.push('\n');
        }
        text
    }

    pub fn decode(text: &str) -> Result<Self, JournalError> {
        let mut lines = text.lines();
        if lines.next() != Some(LEDGER_HEADER) {
            return Err(JournalError::Corrupt);
        }
        let mut ledger = Self::default();
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != LEDGER_FIELDS {
                return Err(JournalError::Corrupt);
            }
            opaque(fields[0]).map_err(|_| JournalError::Corrupt)?;
            let entry = LedgerEntry {
                owner: optional(fields[1]).map_err(|_| JournalError::Corrupt)?,
                state: LedgerState::parse(fields[2]).ok_or(JournalError::Corrupt)?,
            };
            if ledger.entries.insert(fields[0].to_owned(), entry).is_some() {
                return Err(JournalError::Corrupt);
            }
        }
        Ok(ledger)
    }
}

/// External manifest of a managed backup. Membership is opaque local
/// identifiers only; the manifest never lists provider identifiers or usage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupManifest {
    pub backup_id: String,
    pub schema_version: i64,
    pub created_at: DateTime<Utc>,
    pub included_ids: Vec<String>,
}

impl BackupManifest {
    pub fn validate(&self) -> Result<(), JournalError> {
        opaque(&self.backup_id)?;
        if self.schema_version < 0 {
            return Err(JournalError::Validation);
        }
        for id in &self.included_ids {
            opaque(id)?;
        }
        Ok(())
    }
}

/// One step recovery owes before connections may be scheduled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    /// Delete an opaque secret reference that cleanup still owes: an orphan
    /// candidate from an uncommitted operation, or a superseded reference after
    /// a commit.
    DeleteReference {
        operation_id: String,
        connection_id: String,
        reference: String,
    },
    /// Release the database fence of an operation whose owed references are gone.
    FinishOperation {
        /// Merged record whose fence to release; its phase is the pre-completion
        /// one, so the projection is still incomplete when recovery reads it.
        record: OperationRecord,
    },
    /// A deletion was never acknowledged. It stays fenced, visible, and retryable;
    /// recovery does not delete credentials for a deletion that did not commit.
    DeletionPending { opaque_id: String },
}

impl RecoveryAction {
    /// Connection the action belongs to, when it is operation-scoped.
    pub fn connection_id(&self) -> Option<&str> {
        match self {
            Self::DeleteReference { connection_id, .. }
            | Self::FinishOperation {
                record: OperationRecord { connection_id, .. },
            } => Some(connection_id),
            Self::DeletionPending { .. } => None,
        }
    }
}

/// Durable, atomically replaced files outside the backed-up database.
///
/// Implementations must write a same-directory temporary file, flush and sync
/// it, replace the target atomically, and sync the directory where the platform
/// supports it. A failure means durability was not established, and the caller
/// must stop before writing a secret or acknowledging a deletion.
pub trait DurableStore: Send + Sync {
    /// Current contents, or `None` when the file does not exist yet.
    fn read(&self, name: &str) -> Result<Option<String>, JournalError>;
    /// Replace the whole file durably.
    fn replace(&self, name: &str, contents: &str) -> Result<(), JournalError>;
}

/// Why an opaque secret reference could not be written or deleted. The port
/// never carries secret values, only references; the value-carrying supertrait
/// and its native and session-only stores live in `crate::secrets`.
pub trait SecretReferenceStore: Send + Sync {
    /// Create a candidate reference. Called only after the intent is durable.
    fn write_candidate(&self, reference: &str) -> Result<(), SecretStoreError>;
    /// Delete a reference. Deleting an absent reference must succeed so recovery
    /// can resume cleanup idempotently.
    fn delete_reference(&self, reference: &str) -> Result<(), SecretStoreError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretStoreError {
    /// The store is locked: show a pending action, never declare success and
    /// never downgrade to session-only or plaintext storage.
    Locked,
    Unavailable,
}

impl From<SecretStoreError> for JournalError {
    fn from(error: SecretStoreError) -> Self {
        match error {
            SecretStoreError::Locked => JournalError::Locked,
            SecretStoreError::Unavailable => JournalError::Unavailable,
        }
    }
}

impl From<StorageError> for JournalError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Conflict => Self::Conflict,
            StorageError::Unavailable => Self::Unavailable,
            StorageError::Validation
            | StorageError::Migration
            | StorageError::IncompatibleVersion => Self::Validation,
        }
    }
}

/// Decide what recovery owes from durable external state plus the database
/// projection of the same operations. Pure and side-effect free: the database
/// projection never replaces the external journal, it only says whether a
/// mutation committed.
///
/// `projections` are the database rows whose phase is not
/// [`OperationPhase::Complete`], across every connection.
pub fn reconcile(
    journal: &Journal,
    ledger: &Ledger,
    projections: &[OperationRecord],
) -> Vec<RecoveryAction> {
    let mut actions = Vec::new();
    let mut operations: Vec<OperationRecord> = journal.records().to_vec();
    for projection in projections {
        if projection.phase.fences()
            && !operations
                .iter()
                .any(|r| r.operation_id == projection.operation_id)
        {
            // The journal is missing or behind; the projection is still evidence
            // that cleanup may be owed, so it is never treated as finished.
            operations.push(projection.clone());
        }
    }
    operations.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));

    for record in &operations {
        let committed = record.phase.committed()
            || journal
                .get(&record.operation_id)
                .is_some_and(|r| r.phase.committed())
            || projections
                .iter()
                .find(|p| p.operation_id == record.operation_id)
                .is_some_and(|p| p.phase.committed());
        // After a commit the candidate is the active reference, so only the
        // superseded one is owed. Before a commit the candidate is an orphan.
        let owed = if committed {
            record.previous_ref.clone()
        } else {
            record.candidate_ref.clone()
        };
        if let Some(reference) = owed {
            actions.push(RecoveryAction::DeleteReference {
                operation_id: record.operation_id.clone(),
                connection_id: record.connection_id.clone(),
                reference,
            });
        }
        // Every merged operation still owes its fence release: the journal keeps
        // no completed records, and only non-complete projections are merged in.
        // Completing an already-complete projection is a no-op, so this stays
        // convergent when recovery is interrupted and run again.
        actions.push(RecoveryAction::FinishOperation {
            record: record.clone(),
        });
    }

    for (id, entry) in ledger.entries() {
        if entry.state == LedgerState::PendingCleanup {
            actions.push(RecoveryAction::DeletionPending {
                opaque_id: id.clone(),
            });
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(operation_id: &str, connection_id: &str, phase: OperationPhase) -> OperationRecord {
        OperationRecord {
            operation_id: operation_id.to_owned(),
            connection_id: connection_id.to_owned(),
            kind: OperationKind::Setup,
            phase,
            candidate_ref: Some("synthetic-reference".to_owned()),
            previous_ref: None,
            generation: 0,
        }
    }

    #[test]
    fn a_reused_identifier_for_another_connection_is_refused_before_persistence() {
        let mut journal = Journal::default();
        journal
            .record(record(
                "synthetic-operation",
                "synthetic-connection",
                OperationPhase::Pending,
            ))
            .unwrap();

        // Decoding rejects a repeated operation identifier, so persisting one would
        // leave external state that no coordinator could read again.
        assert_eq!(
            journal
                .record(record(
                    "synthetic-operation",
                    "synthetic-other",
                    OperationPhase::Pending
                ))
                .unwrap_err(),
            JournalError::Conflict
        );
        assert_eq!(journal.records().len(), 1);
        assert_eq!(Journal::decode(&journal.encode()).unwrap(), journal);

        // The owning connection may still advance its own record.
        journal
            .record(record(
                "synthetic-operation",
                "synthetic-connection",
                OperationPhase::CandidateWritten,
            ))
            .unwrap();
        assert_eq!(
            journal.get("synthetic-operation").unwrap().phase,
            OperationPhase::CandidateWritten
        );
        assert_eq!(Journal::decode(&journal.encode()).unwrap(), journal);
    }
}
