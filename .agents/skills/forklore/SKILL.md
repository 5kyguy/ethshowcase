---
name: forklore
description: "Research past ETHGlobal hackathons: every project ever submitted (19,000+ across 79 finished events, 2019–2026), winners and prize placements, descriptions, tech writeups, demo/repo links. Use when mentoring at or preparing for a hackathon, checking prior art ('has X been built at an ETHGlobal?'), listing what won at a specific event, analyzing trends by year or sponsor track, or finding reference projects to learn from. Covers completed hackathons only — never the currently-running event."
---

# ETHGlobal Showcase Research

Query the full ETHGlobal Showcase dataset: 19,002 projects across 79 finished
hackathons (2019 → Sep 2026), with winners, prize placements, descriptions,
how-it's-made writeups, and links. Built for hackathon mentoring: prior-art
checks, winner deep-dives, and trend questions.

Skill version 1.1.0 (renamed from ethshowcase). Dataset freshness is in `stats.json` → `as_of` (data
snapshot) — check it before making claims.

## Hard rule — past hackathons only

The dataset contains **only hackathons that finished before its cutoff**
(`stats.json` → `cutoff`). The currently-running event and its tracks are
excluded by design. If asked about a live event ("what are the tracks this
time?", "who's winning?"), say the dataset is past-only and point to
https://ethglobal.com/events. Do not approximate live answers from old data.

## When to use

- "Has an on-chain RPG / restaking marketplace / ZK identity thing ever been
  built at an ETHGlobal?" (prior art)
- "What won at HackFS 2023 / ETHOnline 2024 / <any event>?" (winners)
- "Show me finalist projects using account abstraction" (filtered search)
- "Which sponsor tracks give pool prizes vs ranked prizes?" (prize data)
- "What did people build with <stack> in 2025 vs 2023?" (trends)

## When NOT to use

- Anything about a currently-running or upcoming hackathon (see hard rule).
- Team/judge scores of a live judging round (data has no judge data at all —
  only ETHGlobal's auto-scores on old projects, mostly empty).

## Data access — three tiers

Start at Tier 1 (zero install). Tier 2 for heavy/repeated analysis. Tier 3 if
the repo is checked out locally. Full copy-paste recipes with output shapes:
**`references/data-tiers.md`** — read it before your first query session.

| Tier | Setup | Best for |
| --- | --- | --- |
| 1. GitHub Pages JSON + `jq` | none | quick lookups, winners lists, prior-art checks |
| 2. SQLite release (FTS5) | download `forklore.db` | ranked full-text search, aggregations |
| 3. Local API (`forklore serve`) | repo checkout | everything, fastest iteration |

Key facts: `search.json` is one array of 19k compact records with letter keys
(`h` = pre-lowercased search haystack; legend in data-tiers.md). Detail files
live at `data/p/{id}.json`. The events chronology is `events.json` — sort by
`start_time`, never trust memory for dates or edition order.

## How to answer

**Conversational (default):** targeted queries, bullet answers, cite as you go.

**Deep dive (explicit opt-in only** — "deep dive", "full analysis", "vet this
idea"): prior-art sweep (multiple query angles), adjacent winner comparison,
trend timeline, gap assessment. Still evidence-bound — no speculation beyond
the corpus.

### Required quality checks

- **Chronology from data, never memory.** Event dates/order come from
  `events.json` (or `/api/events`). Include month/year when naming events.
- **Date every judgment.** Prefix evaluative claims with the dataset's
  `as_of` date ("As of 2026-11: …").
- **Corpus honesty.** "No matches" means no matches in ~19k ETHGlobal
  projects — not "nobody ever built it". Say so when answering prior-art
  questions, and widen the query (synonyms, adjacent terms) before concluding.
- **Cite projects inline** by slug and link `showcase_url`. For winners,
  name the exact prize placement ("1st place", sponsor + title).
- **Finalist ≠ winner.** The `finalist` flag = ranked placement (1st/2nd/3rd
  or finalist/winner text) in any track. See `references/winners-and-prizes.md`
  before making claims about who "won" — pool prizes are participation-scale,
  not wins.

### Output contract

- Bullets, not essays; 5–15 bullets typical for conversational answers.
- Inline citations: `project-slug` (+ showcase_url), event name + month/year,
  prize placement text as stored.
- No separate sources section — citations live inline.
- State the tier used and the `as_of` date once per answer.

## Attribution

Project content belongs to its authors; every record carries `showcase_url`
back to the original showcase page. When reusing project descriptions, credit
the team and link the showcase page. Dataset by 5kyguy, scraped from the
public ETHGlobal showcase.

## References

- `references/data-tiers.md` — access tiers, letter-key legend, tested recipes
- `references/query-recipes.md` — answers for common mentoring questions
- `references/winners-and-prizes.md` — finalist semantics, prize shapes, pitfalls
