# ethshowcase

Scrape, store, and serve the **complete ETHGlobal Showcase project database** —
every project ever submitted, across all events — as a fast, queryable dataset
for hackathon participants.

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
runs never re-hit ETHGlobal. Requests are paced, concurrency-capped, and
retried with exponential backoff — be a good citizen.

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

# 3. Build SQLite (FTS5) + static site
ethshowcase build --site site/dist

# 4. Query API on :8080
ethshowcase serve
```

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
| `build` | JSONL → SQLite (FTS5), optionally `--site <dir>` |
| `serve` | HTTP API over the built DB |
| `stats` | Dataset statistics |
| `inspect` | Parse a saved HTML file (fixture debugging) |

Common flags: `--concurrency` (default 8), `--interval-ms` (default 60),
`--force` (refetch details), `--fresh` (bypass raw-HTML cache),
`--max-pages` (test runs).

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
scripts/deploy-site.sh          # → GitHub Pages (gh-pages branch)
# or: wrangler pages deploy site/dist   (Cloudflare Pages)
scripts/release-dataset.sh v2026.11   # dataset snapshot as a GH release
```

The API (`ethshowcase serve`) is a single static binary — deploy it anywhere
(Fly.io, a VPS, or Tailscale-only during the event) with the `ethshowcase.db`
file.
