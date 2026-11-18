// Lance les tests d'intégration SSH (tests Rust marqués #[ignore]).
//
// En local, les variables YOUNJI_TEST_* peuvent être placées dans .env.test (ignoré par git,
// voir docs/ENVIRONNEMENT_DE_TEST.md) : ce script le charge s'il existe. En CI, elles sont
// fournies par le workflow. Les valeurs ne sont jamais affichées.
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';

if (existsSync('.env.test')) {
  process.loadEnvFile('.env.test');
}

const result = spawnSync('cargo', ['test', '--workspace', '--', '--ignored'], {
  stdio: 'inherit',
  env: process.env,
});
if (result.error) {
  console.error(`Impossible de lancer cargo : ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
