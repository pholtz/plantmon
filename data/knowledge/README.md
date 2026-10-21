# Plant knowledge data

This directory is a source-backed pilot for richer plant guides. It is separate
from `catalog.tsv` and from mutable collection data in SQLite.

The initial profiles cover five taxa and the families and care groups referenced
by the development collection:

- `Chlorophytum comosum`
- `Pachira aquatica`
- `Aphelandra squarrosa`
- `Goeppertia insignis`
- `Goeppertia warszewiczii`

The database was used only to choose subjects. Nicknames, locations, schedules,
journal entries, and other collection data are not copied into these files.

## Files

- `requirements.toml`: reviewed exposure tiers and numeric environment targets used
  by opt-in assessments.
- `sources.toml`: reusable source metadata.
- `taxa.toml`: species-level care, background, history, and peculiarities.
- `families.toml`: broader botanical context.
- `care_groups.toml`: horticultural guidance and explicit scope limits.

Every document has a `schema_version`. Every profile identifies its `scope` and
catalog `key`, and every fact has a stable ID, section, topic, paraphrased text,
and one or more source IDs. Profiles carry a review date and content version.


Environment requirements use only explicit structured values: a minimum/maximum
five-level exposure, a bounded preferred Celsius range, and an optional humidity target
(which may be lower-bounded). Every target has its own source IDs. The loader rejects
unknown enum values or catalog keys, reversed/out-of-bounds ranges, missing citations,
malformed dates, and duplicate subjects. Assessment code never derives targets from
`catalog.tsv` light prose or uncited care notes. A care-group fallback is permitted only
when `requirements.toml` contains a separately cited care-group target.
Care groups are not taxonomic ranks. Broad labels such as `trees`, `flowering`,
and `tropical foliage` deliberately avoid universal water or light
prescriptions. Species-level guidance takes precedence over care-group and
family context.

## Source policy

Prefer:

1. Kew Plants of the World Online for accepted names, synonyms, families, and
   native ranges.
2. University extension services, recognized botanical gardens, and the Royal
   Horticultural Society for cultivation.
3. Scholarly or institutional sources for history and unusual biological claims.

Paraphrase rather than copying source prose. Record uncertainty and conflicting
taxonomy directly. A source only supports the facts that cite it; its presence
does not validate a whole profile or the rest of the catalog.

When changing a fact, verify the linked source, update `reviewed_on`, and increase
`content_version` when the meaning changes. `cargo test --locked` rejects unknown
sources, duplicate IDs, malformed dates, unsupported scopes, and profile keys
that do not exist in the bundled catalog.

Use `plantmon --guide SEARCH` to inspect the compiled profiles and their source
trail without opening the TUI.
