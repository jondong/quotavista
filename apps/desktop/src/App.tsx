import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SyntheticStatus {
  provenance: string;
  providerCount: number;
  store: string;
  message: string;
}

export default function App() {
  const [status, setStatus] = useState<SyntheticStatus | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    invoke<SyntheticStatus>("synthetic_status")
      .then(setStatus)
      .catch(() => setFailed(true));
  }, []);

  return (
    <main className="container">
      <h1>QuotaVista</h1>
      <p className="synthetic-banner" role="status">
        Prototype scaffold — synthetic data only. No real provider accounts are
        connected.
      </p>
      {failed && <p>Backend status unavailable.</p>}
      {status && (
        <dl>
          <dt>Provenance</dt>
          <dd>{status.provenance}</dd>
          <dt>Demo providers</dt>
          <dd>{status.providerCount}</dd>
          <dt>Store</dt>
          <dd>{status.store}</dd>
          <dt>Status</dt>
          <dd>{status.message}</dd>
        </dl>
      )}
    </main>
  );
}
