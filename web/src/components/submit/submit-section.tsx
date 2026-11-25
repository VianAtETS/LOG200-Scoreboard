import { useGetHealth } from '@/dao/hooks/use-ladder';
import { submitJar } from '@/dao/scoreboard.repository';
import { useCallback, useState } from 'react';
import type { ChangeEvent, FC, SubmitEvent } from 'react';

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

  const handleTokenChange = useCallback((event: ChangeEvent<HTMLInputElement>): void => {
    setToken(event.target.value);
  }, []);
  const handleFileChange = useCallback((event: ChangeEvent<HTMLInputElement>): void => {
    setFile(event.target.files?.[0] ?? null);
  }, []);
  const submit = useCallback(async (): Promise<void> => {
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
        `Envoi impossible : ${error instanceof Error ? error.message : String(error)}. Vérifiez la connexion, puis réessayez.`,
      );
    }
  }, [file, state, token]);
  const handleSubmit = useCallback(
    (event: SubmitEvent): void => {
      event.preventDefault();
      void submit();
    },
    [submit],
  );

  return (
    <details id="submit-section" className="section submit" hidden={!available}>
      <summary id="submit-title">Soumettre un JAR</summary>
      <form id="submit-form" className="submit-form" onSubmit={handleSubmit}>
        <label className="field" htmlFor="submit-token">
          <span>Token de l&apos;équipe</span>
          <input
            id="submit-token"
            name="token"
            type="password"
            autoComplete="off"
            spellCheck={false}
            required
            value={token}
            onChange={handleTokenChange}
          />
        </label>
        <label className="field file" htmlFor="submit-file">
          <span>Jar</span>
          <input
            id="submit-file"
            name="file"
            type="file"
            accept=".jar"
            required
            onChange={handleFileChange}
          />
          <span className="file-button" aria-hidden="true">
            <span id="submit-file-name">{file?.name ?? 'Choisir le jar'}</span>
          </span>
        </label>
        <button id="submit-button" className="primary" type="submit" disabled={state === 'pending'}>
          Envoyer le jar
        </button>
      </form>
      <output id="submit-status" className="submit-status" aria-live="polite" data-state={state}>
        {message}
      </output>
    </details>
  );
};

export { SubmitSection };
