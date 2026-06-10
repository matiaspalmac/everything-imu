export default {
  // Biome owns TS/JS/JSON formatting + lint; only the staged files run.
  "*.{ts,tsx,js,jsx,json}": (files) =>
    `pnpm exec biome check --write --no-errors-on-unmatched ${files.map((f) => `"${f}"`).join(" ")}`,
  // Prettier owns Markdown + YAML.
  "*.{md,yml,yaml}": (files) =>
    `pnpm exec prettier --write ${files.map((f) => `"${f}"`).join(" ")}`,
  // rustfmt on staged Rust files; clippy stays in CI (too slow per-commit).
  "*.rs": (files) => `rustfmt --edition 2021 ${files.map((f) => `"${f}"`).join(" ")}`,
};
