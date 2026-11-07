# Winners and prizes — semantics and pitfalls

## `finalist` is derived, and it means "ranked placement winner"

ETHGlobal's showcase markup has **no boolean finalist field**. The dataset
derives the flag from prize placements — including ETHGlobal's own track,
whose placements read `🏆 <Event> Finalist`:

- `finalist: true` — the project holds a ranked placement in any track:
  `1st place`, `2nd place`, `3rd place`, or placement text containing
  "finalist"/"winner".
- Pool prizes (`Pool Prize`, `🏊 … Pool Prize`, sponsor-title placements like
  `Integrate ENS`) are **participation-scale** — they do NOT set the flag.

So "finalist" ≈ "won a ranked place", the closest public proxy for
stage finalists. ~1,910 of 19,002 projects qualify (as of 2026-11).

**Never say "X won hackathon Y" because X has pool prizes.** Check the
placement strings. Rule of thumb:

| placement text | claim you can make |
| --- | --- |
| `1st place` / `2nd place` / `3rd place` (sponsor track) | "took <place> in <sponsor>'s track at <event>" |
| text containing `finalist` / `winner` | "was a finalist/winner at <event>" |
| `Pool Prize` or a sponsor track title | "was recognized by <sponsor>" / "built for <sponsor>'s bounty" |

## Prize object shape

```json
{
  "sponsor": "Scroll",
  "sponsor_id": "…",
  "placement": "2nd place",
  "title": "Deploy on Scroll",
  "amount": null
}
```

- `placement` may be `null` — then `title` (the bounty name) is the signal.
- Prize data comes from the project page; 8k+ projects have any at all.
- A project can hold several prizes across sponsors.

## Querying prizes

- Placements are **free text**: `1st place`, `Pool Prize`, sponsor-specific
  like `AgentKit Pool Prize`, emoji-prefixed like `🏊 Scroll — Pool Prize`.
  Substring-match, lowercase both sides. Don't enumerate placements from
  memory — pull the distribution:

```sh
curl -s https://forklore.skyguy.dev/data/search.json \
  | jq -r '[.[] | .z[]?] | map(ascii_downcase) | group_by(.) | map({p: .[0], n: length})
           | sort_by(-.n) | .[:12][] | "\(.n)  \(.p)"'
```

- The API's `prize=` param matches placement **or** sponsor, substring,
  case-insensitive (`prize=worldcoin`, `prize=1st%20place`).
- Ranked-only winner lists: filter `finalist == true` (or `.f` in search.json).

## Event naming pitfalls

- Slugs ≠ years: `hackmoney2026` ran Jan–Feb 2026; `ethonline2025` Oct 2025.
  Always resolve slugs/dates via `events.json` or `/api/events`.
- Same franchise, different editions: `ethnyc` (2020, cancelled — excluded
  from the dataset), `newyork2026`. Never guess from the name.
- The year filter uses the event's **start** year.
