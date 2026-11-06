# ethshowcase

Scrape, store, and serve the **ETHGlobal Showcase project database** — every
project from completed hackathons (19,002 projects across 79 events,
2019–2026) — as a fast, queryable dataset for hackathon participants, plus an
installable agent skill for researching past hackathons.

Built in Rust on top of the site's server-rendered flight payload: no browser
automation, no fragile CSS scraping of the data itself.

## How it works

ETHGlobal's showcase is a server-rendered Next.js app. Every listing page
embeds the full structured record of its 33 projects (title, tagline, links,
media, prizes, event, auto-scores) in the RSC flight payload, and each project
page adds the long-form `description` / `how it's made` texts. So:

1. `index` — walk `/showcase?page=N` until it stops yielding projects
   (~440 requests for ~14,500 projects).
2. `details` — fetch each project page once for the long texts.
3. `build` — persist to SQLite with FTS5 full-text search, and/or emit a
   zero-dependency static search site.
4. `serve` — queryable HTTP API over the database.

Every raw HTML response is cached gzipped on disk, so re-parses and interrupted
runs never re-hit ETHGlobal. Requests are paced (default ~3 req/s ceiling,
2 concurrent), retried with exponential backoff, and the client adapts
automatically to rate limiting: each 429 doubles the global request spacing
and starts a shared quiet period (15 s doubling up to 5 min); sustained
success slowly recovers pace. Identifying User-Agent — be a good citizen.

## Install / build

```sh
cargo build --release
# binary: target/release/ethshowcase
```

## Usage

```sh
# 1. Full index (cached; ~440 requests)
ethshowcase index

# 2. Detail pages for description/how-it's-made (~10-20 min, resumable)
ethshowcase details

# 3. Build SQLite (FTS5) + static site (past hackathons only by default)
ethshowcase build --site site/dist

# 3b. Keep live/current-event data instead (not recommended for the
# published dataset — the current hackathon's tracks and prizes churn)
ethshowcase build --site site/dist --include-live

# 4. Query API on :8080
ethshowcase serve
```

> **Past-only builds.** `build` keeps only events that finished before the
> build time: cancelled events and the currently-running hackathon's tracks
> (whose slugs appear only on project pages) are excluded and stale rows are
> pruned. The applied rule is recorded in `stats.json` → `cutoff`, and
> `as_of` carries the data snapshot date.

> **Note on `finalist`:** ETHGlobal's showcase has no finalist badge; the flag
> marks ranked placement winners (1st/2nd/3rd place in any track, or
> finalist/winner placements). 1,910 of 19,002 past-only projects qualify. Use
> the `prize=` filter for sponsor-specific and pool-prize queries.

### API

```
GET /api/projects?q=uniswap&event=ethonline2025&year=2025&finalist=true&prize=1st&limit=50&offset=0
GET /api/projects/:id            # id or "slug-id"
GET /api/events
GET /api/stats
GET /healthz
```

### Static site

`site/dist` is a fully static, zero-dependency search UI (client-side search
over titles/taglines/summaries, event/year/finalist filters, per-project detail
views). Drop it on GitHub Pages, Cloudflare Pages, or any static host.

### Agent skill

`.agents/skills/ethshowcase/` packages the dataset as an agent skill (SKILL.md
+ tested query recipes): any OpenCode / Claude Code / Codex session can then
answer "what won at HackFS 2023?", "has X been built before?", or trend
questions from this dataset — past hackathons only.

Install (cross-platform, via the [skills CLI](https://github.com/vercel-labs/skills);
adds it to every agent runtime it finds):

```sh
npx skills add 5kyguy/ethshowcase
```

No Node? Plain git works everywhere:

```sh
git clone --depth 1 https://github.com/5kyguy/ethshowcase /tmp/ethshowcase
mkdir -p ~/.config/opencode/skills
cp -r /tmp/ethshowcase/.agents/skills/ethshowcase ~/.config/opencode/skills/
```

The skill works with zero local setup (GitHub Pages JSON + `jq`), the
[SQLite release](https://github.com/5kyguy/ethshowcase/releases) (FTS5), or a
local `ethshowcase serve` instance — see `.agents/skills/ethshowcase/references/`.

### Debugging the parser

```sh
ethshowcase inspect saved-page.html          # table of extracted projects
ethshowcase inspect --json saved-page.html   # full resolved records
```

## CLI reference

| Command | Purpose |
| --- | --- |
| `index` | Walk listing pages → `data/projects.index.jsonl` + `data/events.json` |
| `details` | Fetch project pages, merge descriptions → `data/projects.jsonl` |
| `build` | JSONL → SQLite (FTS5), optionally `--site <dir>`; past-only by default, `--include-live` keeps current-event data |
| `serve` | HTTP API over the built DB |
| `stats` | Dataset statistics |
| `inspect` | Parse a saved HTML file (fixture debugging) |

Common flags: `--concurrency` (default 2), `--interval-ms` (default 350,
adaptive slowdown applies on top), `--force` (refetch details), `--fresh`
(bypass raw-HTML cache), `--max-pages` (test runs).

## Ethics & legality notes

- Scrapes only **publicly accessible** pages, at a polite, paced rate with an
  identifying User-Agent.
- Every dataset record keeps a `showcase_url` linking back to the original
  ETHGlobal page; project content belongs to its authors.
- The raw-HTML cache keeps load on ETHGlobal minimal — re-runs after parser
  fixes never re-download.
- Intended for hackathon community/mentoring use. Don't resell the data.

## License

MIT

## Hosting

```sh
ethshowcase build --site site/dist
scripts/deploy-site.sh          # pushes gh-pages + dispatches the Actions deploy
# or: wrangler pages deploy site/dist   (Cloudflare Pages)
scripts/release-dataset.sh v2026.12   # dataset snapshot as a GH release
```

Pages deploys via the `deploy-site` GitHub Actions workflow (artifact-based);
the legacy branch builder proved unreliable for the ~19k-file tree. The API
(`ethshowcase serve`) is a single static binary — deploy it anywhere (Fly.io,
a VPS, or Tailscale-only during the event) with the `ethshowcase.db` file.
