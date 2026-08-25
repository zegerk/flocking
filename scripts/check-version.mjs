import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { readVersionState, stateErrors } from './version-utils.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

try {
  const state = await readVersionState(root);
  const errors = stateErrors(state);
  if (errors.length) {
    for (const error of errors) console.error(error);
    process.exitCode = 1;
  } else {
    console.log(`version ${state.version} is synchronized`);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}