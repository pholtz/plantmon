# plantmon

A local-first houseplant collection, care calendar, and journal in Rust + Ratatui.
SQLite remembers your plants, greenhouse order, and maintenance; nothing needs a
network connection.

## Run

Requires Rust 1.90+ and a Unicode / true-color terminal (Kitty works well).
SQLite is bundled; no database server or system SQLite headers are needed.

```sh
cd plantmon
cargo run --release --locked

# Explore eight example plants without touching your collection:
cargo run --release --locked -- --demo

# Optional installation:
cargo install --path . --locked
```

Start with `a`, search for a plant, and choose it with Enter. Type any details,
advance with Enter or Down, then select **Save** and press Enter. All detail fields
are optional. Press `?` on any screen for its contextual help and command menu.

## The greenhouse

The main view is a responsive grid of plant cards: botanical identity, location,
Unicode artwork, an estimated water meter, and care reminders. It shows as many
plants as fit and pages the remainder. The selected plant has a bright border.
Enter or Space focuses that specimen, including its attributes, all schedules,
care notes, and a chronological maintenance journal. Navigation still works in
the detail view. Press `o` for a list of every plant, pick one up with Enter or
Space, move it with the arrow keys, and drop it to save a persistent manual order.
Pressing `g` still cycles through name, family, care-group, location, and manual
sorting.

The header stays on one line: app name and a muted green plant count on the
left, the current view / sorting centered, and a sun/moon icon plus local timestamp
right-aligned. The tagline disappears when space is tight; very small terminals
use a compact app label and time-only clock. Reminders stay on each plant card.
Water estimates use a quiet foreground block bar rather than a filled background.
Cards use relaxed spacing by default when height permits; compact spacing targets
shorter rows and can show several rows before paging.

Artwork covers nine growth habits, including fenestrated monstera leaves, vines,
cacti, palms, ferns, rosettes, and orchids. Other species inherit their habit's
silhouette; unknown habits fall back to a generic leafy plant. These are stylized
Unicode illustrations, not photos; Kitty's image protocol is not required.

### Controls

Every screen keeps a single, quiet footer: **`? help | q quit`**. Keyboard
reference lists live only in help, not in panel titles, status messages, or
form footers. Notifications share that line when they fit, fall back above it
when they do not, and disappear after five seconds. Input fields and Save/Cancel
controls remain visible where needed.

`?` opens help for the current page: a title, a short summary, and a neatly
aligned command list. Use Up/Down to select a command and Enter to run it;
PgUp/PgDn and Home/End move through longer lists. `?` or Esc returns to exactly
where you were, preserving search text, form values, focus, and confirmations.
`q` quits from help; Ctrl-C quits everywhere. In active text inputs, `q` remains
a normal letter so names and notes can contain it. Leave the input before using
`q` to quit, or choose Quit from help.

Help opens on the **Commands** tab. Tab / Shift-Tab switches between Commands
and **Settings**, without losing your command selection or an unfinished form.

The original action-specific shortcuts remain available:

| Key | Action |
| --- | --- |
| Arrows / `hjkl` | Move between cards; in details, move between plants |
| Tab / Shift-Tab | Next / previous plant, wrapping around |
| Enter / Space | Toggle focused detail view |
| Home / End | First / last plant |
| PgUp / PgDn | Page the grid; scroll the focused specimen or help |
| `a` | Add a plant from the catalog |
| `/` | Filter by botanical/common name, family, care group, location, or tags |
| `g` | Sort by name → family → primary care group → location → manual order |
| `o` | Reorder the greenhouse in a keyboard-controlled list |
| `d` | Toggle plants with due care reminders only |
| `w` | Record watering **now** |
| `r` / `f` / `i` | Record rotation / fertilizing / inspection **now** |
| `W` / `m` | Log any maintenance with an optional date and note |
| `t` | Set individual care intervals; **0 disables** a task |
| `e` | Edit nickname, location, acquired date, pot, substrate, tags, notes |
| `u` | Undo the last-entered maintenance event, after confirmation |
| `D` | Delete the selected plant and its journal, after confirmation |
| `R` | Reload the database immediately |
| `?` | Open contextual help / command menu |
| Esc | Cancel dialog, leave detail view, or clear collection filters |
| `q` / Ctrl-C | Quit (`q` stays text inside active text inputs) |

In the catalog, type to search, use arrows / PgUp / PgDn to select, and Enter
to choose. A **Custom plant type** option is always below the results and is
selected automatically when there are no matches. Select it with arrows and
Enter, use End to jump to it, or use the existing Ctrl-N shortcut.

In forms: Enter advances, Up/Down moves between fields and **Save / Cancel**, and
Enter on either control activates it. Left/Right switches between Save and
Cancel, cycles maintenance tasks / growth habits, or adjusts numeric day values.
Typing replaces a prefilled text value on first input; Backspace erases.
Ctrl-S saves directly and Ctrl-U clears the current text field, but neither is
required to finish a form. Esc cancels; bracketed paste is supported. Long forms
follow the focused field. Help preserves an in-progress edit.

Confirmations start with **Cancel** selected. Choose Cancel or Confirm with
arrows and Enter; the existing `y` / `n` shortcuts still work.

### Settings and daylight

Open `?`, then Tab to **Settings**. Up/Down selects a setting; Left/Right or Enter
changes it. Changes apply immediately and are saved automatically in SQLite:

- **Theme**: Default (terminal palette), Greenhouse (initial choice), Light, or
  Dark. Default uses your terminal's background, foreground, and ANSI colors rather
  than fixed RGB values. All screens, dialogs, meters, and artwork follow the theme.
- **Display artifacts**: off by default. When enabled, full-sun plants sparkle
  frequently, part-sun plants less often, and shade plants display a small moon.
  At night, all plants use a moon. Decorations stay outside the plant silhouette.
- **Card spacing**: Relaxed by default, adding vertical breathing room when the
  terminal has space. Compact reduces the target row height and hides card artwork
  as needed so more plants can fit on each page.

An explicit light description in the existing **Location / light exposure** field
(e.g. `Kitchen / full sun`, `part sun`, or `shade`) overrides the catalog light
preference for decorations. Without one, catalog light preference is used; an
unknown preference defaults to part sun. This is cosmetic, not measured exposure,
and never changes maintenance schedules or water estimates.

The header's **☀ / ☾** indicates estimated daylight / nighttime. Plantmon uses the
current local date and time: **6am inclusive to 6pm exclusive** is daytime. The
state is recomputed every tick and on startup, including after sleep. No weather,
location lookup, seasonal sunrise calculation, or network access is involved.

### Maintenance and backdating

`W` / `m` opens a form with task, date, and note. Choose the task with Left/Right:
water, rotate, fertilize, soil, or inspect. Enter advances to the date and note,
then to Save. Examples for the date:

- `now` or blank
- `yesterday`, `2d`, `12h`, `30m` — elapsed time before now
- `2026-10-12` — local midnight
- `2026-10-12 18:30` — local date and time

Future dates, negative durations, and ambiguous/nonexistent daylight-saving times
are rejected. Old entries are inserted into the journal in actual-time order.
Timers always use the **latest actual completion**, so entering a forgotten older
watering does not overwrite a more recent one. Undo removes the last *entered*
event for that plant, even when that entry was backdated, and recalculates timers.

## Care model

Timers use persisted UTC timestamps, not process uptime. The UI checks wall-clock
time every second, including after machine sleep. Startup immediately recomputes
all due dates. Reminders remain due until you log completion or disable them;
missing ten rotations does not create ten duplicate tasks. The database reloads
every 15 seconds so another process's changes become visible.

Defaults are species-specific water **check** intervals, rotation every 14 days,
feeding every 30 days, soil review/refresh every 365 days, and inspection every
7 days. Feeding starts disabled for carnivorous plants. Every task is independently
adjustable per individual plant. Initial non-water reminders start from the day
you add the plant; no fictitious maintenance is added to its journal. New plants
have **unknown hydration** and an immediate water-check reminder.

**Water meters are estimates, not moisture sensors.** After a logged watering,
the meter decreases linearly from 100% to 0% over its configured interval.
Disabled water schedules and plants with no watering history have no estimate.
Check soil and the plant before watering. Light, drainage, pot size, substrate,
season, and dormancy can change care needs drastically. Adjust or disable tasks
manually for seasonal plants. Nothing waters, fertilizes, or replaces soil
automatically. `soil` can mean reviewing or refreshing substrate, not obligatory
annual repotting. No notifications are sent while the program is closed.

Use `--bell` for a terminal bell when an additional task becomes due while running.
Plant cards always show outstanding reminders, without this flag.

## Botanical catalog

`data/catalog.tsv` ships **227 entries**, spanning aroids, ficus, hoyas, palms,
ferns, prayer plants, peperomias, begonias, orchids, bromeliads, succulents,
desert/jungle cacti, and a few specialty carnivorous plants.

Each entry has genus, species, an optional cultivar/hybrid designation, common
names and synonyms, family, overlapping care groups, growth habit, an initial
water-check interval, light preference, and care notes. `Monstera deliciosa` and
`Monstera deliciosa 'Thai Constellation'` are distinct catalog entries; each can
have any number of individual plants. Botanical names, common names, legacy
synonyms such as Sansevieria, families, and all care groups are searchable.
Personal tags let you add your own overlapping groups. Care-group sorting uses
the entry's first group; filtering searches *all* groups.

Unknown hybrid parentage is represented with a genus and designation rather
than an invented species. Explicit nothospecies can include `×` in the species
field. The catalog is a curated horticultural starter collection, **not an
exhaustive or authoritative taxonomic registry, toxicity guide, or care prescription**.
Cultivar trade names and botanical classifications can differ between sources.
Use a custom taxon when yours is missing or needs a more precise identification.

### Catalog provenance and sources

The original **226 entries were AI-generated for this project**, based on general
horticultural knowledge, not imported from an external botanical database. No
per-entry references were recorded for that initial batch. Treat those entries as
unverified starter data, not a source-validated scientific dataset.

Source-checked additions:

| Entry | References and fields checked |
| --- | --- |
| *Aphelandra squarrosa* — zebra plant | [Wikipedia](https://en.wikipedia.org/wiki/Aphelandra_squarrosa): botanical name, common name, family, and light/moisture guidance. [NC State Extension Gardener Plant Toolbox](https://plants.ces.ncsu.edu/plants/aphelandra-squarrosa/): names, family, bright indirect light, high humidity, moist well-drained soil, and reduced winter watering without complete drying. |
| *Goeppertia warszewiczii* — jungle velvet calathea | [Plants of the World Online, Kew Science](https://powo.science.kew.org/taxon/urn:lsid:ipni.org:names:77130098-1): accepted botanical name, *Calathea warszewiczii* synonym, and family. [Wikipedia](https://en.wikipedia.org/wiki/Goeppertia_warszewiczii): botanical identity, synonym, native range, and foliage description. [NC State Extension Gardener Plant Toolbox](https://plants.ces.ncsu.edu/plants/goeppertia-warszewiczii/): common name, family, bright indirect light, moist well-drained mix, humidity, water quality, and temperature guidance. |

These references support this addition, not every entry in the original catalog.
Care notes are paraphrased summaries. Water-check intervals are application-chosen
starting reminders, **not watering schedules prescribed by the references**; adjust
them for your plant and conditions. Future source-checked additions should record
their botanical identity, reference links, and checked fields here.

The TSV is tab-separated, ten columns, one taxon per row. It is compiled into the
binary and validated for duplicate botanical identities and invalid habits or
intervals. Rebuild to ship catalog additions. Startup inserts missing catalog
entries without overwriting existing entries, individual schedules, custom taxa,
or journal entries.

### Plant knowledge pilot

`data/knowledge/` contains an initial source-backed guide for the five taxa,
four families, and six care groups represented in the development collection.
Taxon facts cover care, background, history, and peculiarities. Family profiles
provide botanical context, while broad care-group profiles explicitly avoid
presenting labels such as `trees` or `flowering` as universal care instructions.

Sources, access dates, review dates, content versions, and per-fact citations are
stored with the data. The loader rejects malformed documents, duplicate IDs,
unknown citations, and subjects absent from the catalog. See
`data/knowledge/README.md` for the schema and source policy.

The full TUI guide is still experimental. For now, inspect compiled profiles and
their source trail in plain text:

```sh
plantmon --guide 'jungle velvet'
plantmon --guide 'Marantaceae'
plantmon --guide 'humidity loving'
```

## Storage and command-line tools

Default database: **`scripts/plantmon/data/plantmon.sqlite3`**, resolved from the
build-time project directory, independently of your current working directory.
Database files, WAL/SHM sidecars, and `target/` are gitignored; the catalog and
`Cargo.lock` are not. Changes are saved immediately in SQLite transactions.

For an installed binary, choose a permanent personal path rather than relying
on the source checkout remaining present:

```sh
export PLANTMON_DB="${XDG_DATA_HOME:-$HOME/.local/share}/plantmon/plants.sqlite3"
plantmon

# An explicit flag takes precedence over the environment variable:
plantmon --database /path/to/my-garden.sqlite3

# Report outstanding tasks without opening the TUI:
plantmon --check

# Search the bundled catalog without opening or creating a database:
plantmon --catalog 'hoyas'
plantmon --catalog 'Thai Constellation'
plantmon --catalog ''

# Search source-backed plant knowledge:
plantmon --guide 'prayer plants'
```

`--demo` uses an in-memory database and discards all changes on exit, regardless
of your configured database path. It never seeds examples into your real garden.

To back up, quit all plantmon instances and copy the `.sqlite3` file. While the
app is running, use SQLite's backup API / the `sqlite3` CLI `.backup` command;
do not copy only the main file while WAL transactions are active. Schema version
3 is recorded in `PRAGMA user_version`; newer unsupported schemas are refused
before modification. The version-1 upgrade adds a settings table, and version 3
adds persistent display positions. Both preserve plants, schedules, and journals.
Keep a backup before any schema upgrade.

For this specific version-3 upgrade, rollback to an older schema-2 binary is
additive-safe: stop all instances, back up the database, then run
`sqlite3 /path/to/garden.sqlite3 'PRAGMA user_version = 2;'` before launching the
old binary. It ignores the extra display-order column and manual-order setting;
plants and care records remain usable. Do not apply this procedure to unrelated
future migrations.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

Regression coverage includes out-of-order/backdated care, undo, disabled timers,
restart and settings persistence, manual greenhouse ordering, additive migration,
invalid form preservation, selection through regrouping and due-only filtering,
catalog validation, arrow/Enter-only entry, help state restoration, responsive
headers and minimal footers, theme rendering, daylight boundaries, artifact
frequency / foliage preservation, and dialogs from 20×5 to 120×40.
