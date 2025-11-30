export default {
  '**/*.rs': (filenames) => [
    `cargo fmt -- ${filenames.join(' ')}`,
    'cargo clippy --all-targets --all-features -- -D warnings',
  ],
};
