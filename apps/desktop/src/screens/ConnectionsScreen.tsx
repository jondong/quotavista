import { useState } from "react";
import { useI18n } from "../i18n";

interface DemoConnection {
  alias: string;
  provider: string;
  connected: boolean;
}

// Official provider names are not translated; aliases keep user content.
const INITIAL_CONNECTIONS: DemoConnection[] = [
  { alias: "Demo Kimi", provider: "Kimi", connected: true },
  { alias: "Demo OpenRouter", provider: "OpenRouter", connected: false },
];

export default function ConnectionsScreen() {
  const { t } = useI18n();
  const [connections, setConnections] = useState(INITIAL_CONNECTIONS);
  const [aliasDraft, setAliasDraft] = useState("");

  const addDemoConnection = () => {
    const alias = aliasDraft.trim();
    if (!alias) return;
    setConnections((current) => [
      ...current,
      { alias, provider: "Demo", connected: false },
    ]);
    setAliasDraft("");
  };

  return (
    <section aria-labelledby="connections-heading">
      <h2 id="connections-heading">{t("connections.heading")}</h2>

      <h3>{t("connections.list.heading")}</h3>
      {connections.length === 0 ? (
        <p className="empty">{t("connections.empty")}</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th scope="col">{t("connections.alias.label")}</th>
              <th scope="col">{t("connections.provider")}</th>
              <th scope="col">{t("connections.state")}</th>
            </tr>
          </thead>
          <tbody>
            {connections.map((connection) => (
              <tr key={connection.alias}>
                <td>{connection.alias}</td>
                <td>{connection.provider}</td>
                <td>
                  {connection.connected
                    ? t("connections.state.connected")
                    : t("connections.state.disabled")}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div>
        <label htmlFor="connection-alias">{t("connections.alias.label")}</label>
        <input
          id="connection-alias"
          type="text"
          value={aliasDraft}
          placeholder={t("connections.alias.placeholder")}
          onChange={(event) => setAliasDraft(event.target.value)}
        />
        <button type="button" onClick={addDemoConnection}>
          {t("connections.alias.add")}
        </button>
        <p className="note">{t("connections.alias.hint")}</p>
      </div>
    </section>
  );
}
