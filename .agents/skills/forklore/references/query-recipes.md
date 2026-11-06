# Query recipes — mentoring greatest hits

Tested one-liners. Tier 1 uses the downloaded `search.json`/`events.json`
(see data-tiers.md); Tier 3 assumes `forklore serve` on :8080.
Replace the example terms with the user's topic.

## 1. Prior art: "has X been built?"

Run 2–3 angles (synonyms, stack names) before concluding anything:

```sh
jq -r '[.[] | select(.h | test("onchain rpg|role-playing game onchain|rpg game"))] | length' /tmp/search.json
jq -r '[.[] | select((.h | test("rpg")) and .f == true)] | .[] | "\(.e)  \(.s)  —  \(.t)"' /tmp/search.json
```

Answer shape: count, then 3–6 notable hits (winners first), then the honesty
clause — "no matches in ~19k ETHGlobal projects ≠ never built anywhere".

## 2. Winners at an event

```sh
# Resolve the event first (slug + dates from data, not memory).
jq -r '.[] | select((.name | test("HackFS"; "i")) or .slug == "hackfs2023") | "\(.slug)  \(.start_time[:10]) → \(.end_time[:10])  \(.status)"' /tmp/events.json

# Ranked winners:
jq -r '[.[] | select(.e == "hackfs2023" and .f == true)] | .[] | "\(.s)  —  \(.t)  [\(.z | join("; "))]"' /tmp/search.json

# API equivalent, with full prize detail:
curl -s 'http://127.0.0.1:8080/api/projects?event=hackfs2023&finalist=true&limit=100' \
  | jq -r '.items[] | "\(.slug) — \(.title) \(.prizes | map("\(.sponsor): \(.placement)") | join(" | "))"'
```

## 3. Sponsor track recon: "what did Scroll / Worldcoin / … reward?"

```sh
jq -r '[.[] | select(.z | join(" ") | test("scroll"; "i"))] | length' /tmp/search.json
curl -s 'http://127.0.0.1:8080/api/projects?prize=scroll&limit=5' | jq '.total, [.items[].title]'
```

Compare ranked vs pool: winners-only with `finalist=true` shows what the
sponsor actually ranked.

## 4. Trends: "what got built in 2025 vs 2023?"

```sh
# Volume per year.
jq -r 'group_by(.d) | .[] | "\(.[0].d)  \(length)"' /tmp/search.json

# Topic share by year (example: ZK).
jq -r 'group_by(.d) | .[] | "\(.[0].d)  \([.[] | select(.h | test("zk|zero knowledge"))] | length)"' /tmp/search.json

# SQL, faster for many cuts:
sqlite3 forklore.db "SELECT strftime('%Y', event_start) y, COUNT(*) n,
  SUM(CASE WHEN description LIKE '%zero knowledge%' OR description LIKE '%zk%' THEN 1 ELSE 0 END) zk
  FROM projects GROUP BY y ORDER BY y;"
```

## 5. Reference projects to learn from

Winners with writeups and open code, for a given topic:

```sh
curl -s 'http://127.0.0.1:8080/api/projects?q=payment+channel&finalist=true&limit=10' \
  | jq -r '.items[] | select(.source_url != null and .how_its_made != null)
           | "\(.slug) — \(.title)\n  \(.source_url)\n  \(.showcase_url)"'
```

## 6. Deep-dive prior-art sweep (opt-in mode)

1. Three query angles via FTS (`q=`), collect top ~10 each.
2. Winner-only pass on the same angles (`&finalist=true`).
3. Sponsor pass: which sponsors rewarded adjacent work (`prize=`)?
4. Timeline: hits per year — rising, fading, or crowded forever?
5. Gap statement: what the corpus does NOT show, dated "As of <as_of>", with
   the corpus-bound honesty clause.
