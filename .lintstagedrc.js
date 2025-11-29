export default {
  'frontend/**/*.{ts,tsx}': (filenames) => [
    'cd frontend && npm run typecheck',
    'cd frontend && npm run lint:fix',
    'cd frontend && npm run format',
  ],
  'frontend/**/*.css': (filenames) => ['cd frontend && npm run format'],
  'backend/**/*.rs': (filenames) => [
    'cargo fmt --manifest-path backend/Cargo.toml --',
    'cargo clippy --manifest-path backend/Cargo.toml --all-targets --all-features -- -D warnings',
  ],
}
