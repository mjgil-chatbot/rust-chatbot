# Pricing Computation

This document describes the current summary-cost normalization behavior in
`src/data/provider_normalization.rs`.

## Current Behavior

- **Gemini**: if a loaded summary has no `total_cost_usd`, the app computes it
  from token counts with `compute_gemini_cost()`.
- **Codex / Nvidia**: normalization focuses on rollout metadata, turn-history
  repair, and context-window fields; existing cost data is preserved.
- **Claude / MiniMax / Qwen / OpenRouter**: loaded summaries are left
  unchanged by the normalization pass.

## Gemini Cost Inputs

`compute_gemini_cost()` uses:

- `input_tokens`
- `cache_read_tokens`
- `output_tokens`
- the normalized model string

Cached input is charged at a lower rate than uncached input. Unknown Gemini
model strings currently return `0.0` so the UI does not invent pricing.

## Gemini Rates (Per Million Tokens)

| Model family | Input | Cached input | Output |
| --- | --- | --- | --- |
| `gemini-3.1-pro` / `gemini-3-pro` up to 200k input tokens | $2.00 | $0.20 | $12.00 |
| `gemini-3.1-pro` / `gemini-3-pro` above 200k input tokens | $4.00 | $0.40 | $18.00 |
| `gemini-3.1-flash` / `gemini-3-flash` | $0.50 | $0.05 | $3.00 |

## Maintenance

- If Gemini pricing changes, update `compute_gemini_cost()` in
  `src/data/provider_normalization.rs`.
- If another provider needs cost derivation instead of pass-through summary
  data, document that provider here when the normalization logic lands.
