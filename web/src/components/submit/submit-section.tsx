import { useGetHealth } from '@/dao/hooks/use-ladder';
import { submitJar } from '@/dao/scoreboard.repository';
import { useState } from 'react';
import type { FC, SubmitEvent } from 'react';

type TSubmitState = 'pending' | 'ok' | 'error';

const INTRO =
  "Deux parties de test contre un bot aléatoire, jusqu'à deux minutes. Si elles se terminent sans dépassement de temps ni plantage, ce jar remplace celui de l'équipe.";

// Le formulaire n'apparaît que si le service de soumission répond (absent en hébergement statique).
const SubmitSection: FC = () => {
  const { data: available } = useGetHealth();
  const [token, setToken] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [state, setState] = useState<TSubmitState>();
  const [message, setMessage] = useState(INTRO);

  const handleSubmit = async (e: SubmitEvent) => {
    e.preventDefault();
    if (!file || state === 'pending') {
      return;
    }
    setState('pending');
    setMessage(
      "Parties de test en cours : une à deux minutes, plus si d'autres soumissions attendent…",
    );
    try {
      const verdict = await submitJar(token.trim(), file);
      setState(verdict.ok ? 'ok' : 'error');
      setMessage(verdict.message);
    } catch (error) {
      setState('error');
      setMessage(
        `Envoi impossible : ${(error as Error).message}. Vérifiez la connexion, puis réessayez.`,
      );
    }
  };

  return (
    <details id="submit-section" className="section submit" hidden={!available}>
      <summary id="submit-title">Soumettre un JAR</summary>
      <form id="submit-form" className="submit-form" onSubmit={handleSubmit}>
        <label className="field" htmlFor="submit-token">
          <span>Token de l'équipe</span>
          <input
            id="submit-token"
            type="password"
            autoComplete="off"
            spellCheck={false}
            required
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
        </label>
        <label className="field file" htmlFor="submit-file">
          <span>Jar</span>
          <input
            id="submit-file"
            type="file"
            accept=".jar"
            required
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
          <span className="file-button" aria-hidden="true">
            <span id="submit-file-name">{file?.name ?? 'Choisir le jar'}</span>
          </span>
        </label>
        <button id="submit-button" className="primary" type="submit" disabled={state === 'pending'}>
          Envoyer le jar
        </button>
      </form>
      <p
        id="submit-status"
        className="submit-status"
        role="status"
        aria-live="polite"
        data-state={state}
      >
        {message}
      </p>
    </details>
  );
};

export { SubmitSection };
