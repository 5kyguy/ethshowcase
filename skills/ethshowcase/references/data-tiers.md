# Data access tiers

Base URLs:

- Pages: `https://5kyguy.github.io/ethshowcase`
- Releases: `https://github.com/5kyguy/ethshowcase/releases` (stable latest
  asset links below never need a version number)
- Repo: `github.com/5kyguy/ethshowcase` (clone for Tier 3)

Always start a session by checking freshness:

```sh
curl -s https://5kyguy.github.io/ethshowcase/data/stats.json | jq
# → projects, events, finalists, with_description,
#   as_of (data snapshot date), cutoff (past-only build rule)
```

`as_of` is the honest freshness date. `cutoff` is when live events were
excluded. Answer-date every claim with `as_of`.

---

## Tier 1 — GitHub Pages JSON + jq (zero install)

Files: `data/search.json` (~8 MB, compact array), `data/events.json`,
`data/stats.json`, `data/p/{id}.json` (full detail per project).

### search.json letter-key legend

| key | field | notes |
| --- | --- | --- |
| `i` | id | detail file name: `data/p/{i}.json` |
| `s` | slug | cite this |
| `t` | title | |
| `g` | tagline | may be empty |
| `e` | event slug | e.g. `hackfs2023`, `ethonline2024` |
| `d` | year | from event start, e.g. `"2024"` (string) |
| `f` | finalist | ranked-placement winner (see winners-and-prizes.md) |
| `z` | prizes | array of placement-or-sponsor strings |
| `h` | haystack | title + tagline + auto-summary + event name, **already lowercased** — substring-match here |

### Recipes

```sh
BASE=https://5kyguy.github.io/ethshowcase

# Download once per session, then query locally.
curl -s "$BASE/data/search.json" -o /tmp/search.json
curl -s "$BASE/data/events.json"  -o /tmp/events.json

# Prior-art check: substring search on the haystack.
jq '[.[] | select(.h | test("account abstraction"))] | length' /tmp/search.json

# Prior-art with winners only.
jq '[.[] | select((.h | test("account abstraction")) and .f == true)] | length' /tmp/search.json

# Winners (finalists) of one event, newest first.
jq -r '[.[] | select(.e == "hackfs2023" and .f == true)] | sort_by(.d) | reverse | .[] | "\(.d)  \(.s)  —  \(.t)"' /tmp/search.json

# All projects from one event, with prize strings.
jq -r '.[] | select(.e == "ethonline2024") | "\(.s)\t\(.z | join("; "))"' /tmp/search.json

# Event chronology (never trust memory for dates).
jq -r 'sort_by(.start_time) | .[] | "\(.start_time[:10])  \(.slug)  (\(.name))"' /tmp/events.json

# Trend: projects per year.
jq -r 'group_by(.d) | .[] | "\(.[0].d)  \(length)"' /tmp/search.json

# Full detail for one hit (id from .i).
curl -s "$BASE/data/p/td1aq.json" | jq '{title, tagline, finalist, prizes, demo_url, source_url, showcase_url, description, how_its_made}'
```

Limits: substring only (no ranking/stemming), tagline-length text (full
descriptions are in the detail files). For ranked full-text search over
descriptions + how-it's-made, use Tier 2.

---

## Tier 2 — SQLite release (FTS5, local)

```sh
# ~250 MB, one download; always the latest release:
curl -L -o ethshowcase.db https://github.com/5kyguy/ethshowcase/releases/latest/download/ethshowcase.db
# or with gh:
gh release download -R 5kyguy/ethshowcase -p ethshowcase.db -O ethshowcase.db

# Lighter alternative (~25 MB): one JSON object per line.
curl -L -o projects.jsonl.gz https://github.com/5kyguy/ethshowcase/releases/latest/download/projects.jsonl.gz
```

### Schema essentials

- `projects` — one row per project; searchable text columns and full JSON in
  `data_json` (`SELECT data_json FROM projects WHERE slug = '...'`).
- `projects_fts` — FTS5 over title, tagline, description, how_its_made,
  auto_summary. Join: `projects_fts f JOIN projects p ON p.rowid = f.rowid`.
- `events` — slug, name, kind, start_time, end_time, status.

### Recipes

```sql
-- Ranked full-text search (terms are ANDed; quote each term).
SELECT p.slug, p.title, p.event_name
FROM projects_fts f JOIN projects p ON p.rowid = f.rowid
WHERE projects_fts MATCH '"account" AND "abstraction"'
ORDER BY rank LIMIT 10;

-- Winners of an event.
SELECT slug, title, finalist FROM projects
WHERE event_slug = 'hackfs2023' AND finalist = 1;

-- By year (CAST matters: strftime returns TEXT).
SELECT COUNT(*) FROM projects
WHERE CAST(strftime('%Y', event_start) AS INTEGER) = 2024;

-- Prize/sponsor substring across all projects (placement OR sponsor).
SELECT slug, title, event_name, prizes_json FROM projects
WHERE EXISTS (SELECT 1 FROM json_each(prizes_json) je
  WHERE LOWER(COALESCE(json_extract(je.value,'$.placement'),'')) LIKE '%1st place%');

-- One full project record. Note: db slug has NO id suffix; the combined
-- form from showcase URLs is slug || '-' || id.
SELECT data_json FROM projects WHERE id = 'td1aq';

-- Events chronology.
SELECT start_time, slug, name, status FROM events ORDER BY start_time;
```

---

## Tier 3 — local API (repo checkout)

```sh
git clone git@github.com:5kyguy/ethshowcase.git && cd ethshowcase
cargo build --release
target/release/ethshowcase serve --port 8080   # serves on 127.0.0.1:8080
```

| Endpoint | Purpose |
| --- | --- |
| `GET /api/projects` | search + filters (below) |
| `GET /api/projects/{idOrSlug}` | one full project — by `id` (`td1aq`) or the combined `slug-id` form from showcase URLs (`filify-td1aq`); a bare slug does not resolve (slugs are reused across events) |
| `GET /api/events` | event list with dates + status |
| `GET /api/stats` | counts + `as_of` + `cutoff` |
| `GET /healthz` | liveness |

`/api/projects` params (all optional, combinable):

| param | semantics |
| --- | --- |
| `q` | FTS5 full-text (terms ANDed, ranked) |
| `event` | exact event slug |
| `year` | integer year of event start |
| `finalist` | `true`/`false` — ranked-placement winners |
| `prize` | substring vs placement **or** sponsor (e.g. `1st%20place`, `worldcoin`) |
| `limit` | 1–500, default 50 |
| `offset` | pagination |

```sh
curl -s 'http://127.0.0.1:8080/api/projects?q=account+abstraction&finalist=true&limit=5' | jq '.total, [.items[].title]'
curl -s 'http://127.0.0.1:8080/api/projects?event=ethonline2024&prize=1st%20place' | jq '[.items[] | {slug, prizes}]'
curl -s http://127.0.0.1:8080/api/projects/filify-td1aq | jq '{title, finalist, prizes}'
curl -s http://127.0.0.1:8080/api/events | jq 'length'
```

Note: `total` is the full match count; `count` is this page's size.
