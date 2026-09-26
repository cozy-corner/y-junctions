export default {
  '**/*.{ts,tsx}': (filenames) => [
    'bun run typecheck',
    `eslint --fix ${filenames.join(' ')}`,
    `prettier --write ${filenames.join(' ')}`,
  ],
  '**/*.css': (filenames) => [
    `prettier --write ${filenames.join(' ')}`,
  ],
};
