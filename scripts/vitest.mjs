// Lance Vitest depuis un dossier courant normalisé.
//
// Sous Windows, si le terminal a ouvert le projet avec une lettre de lecteur en minuscule
// (« c:\… », fréquent dans le terminal intégré de VS Code), Vite résout certains modules vers
// leur chemin réel (« C:\… ») : le module vitest est alors chargé deux fois et les tests
// échouent (« failed to find the current suite »). On relance donc Vitest avec la lettre
// de lecteur en majuscule. Les arguments sont transmis tels quels (ex. `npm run test -- --watch`).
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const cwd = process.cwd().replace(/^[a-z]:/, (drive) => drive.toUpperCase());
// Point d'entrée de la CLI de Vitest (champ « bin » de son package.json).
const vitest = join(cwd, 'node_modules', 'vitest', 'vitest.mjs');
const args = process.argv.slice(2);

const result = spawnSync(process.execPath, [vitest, ...(args.length > 0 ? args : ['run'])], {
  cwd,
  stdio: 'inherit',
});
process.exit(result.status ?? 1);
