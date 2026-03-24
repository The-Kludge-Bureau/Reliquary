# Reliquary

A TurtleWoW DLL plugin that gives addon and mod authors direct access to DBC
game data from Lua. It exposes `RQ_*` globals for looking up rows in any
supported DBC by numeric ID, and follows the same loader convention as
[nampower](https://gitea.com/avitasia/nampower): the DLL exports a `Load()`
function called by the TurtleWoW plugin loader on startup.

DBC files are read directly from the game's MPQ archives on first access and
cached for the lifetime of the session. Patch MPQs are respected: when
multiple archives contain the same DBC file, the highest-priority one wins.

## Lua API

### Generic lookup

```lua
local row, err = RQ_GetRow(dbc_name, id [, locale])
```

Looks up `id` in any supported DBC by name. Returns the row as a table on
success, or `nil` plus an error string on failure. The optional `locale`
argument controls how localized string fields are presented (see
[Locale handling](#locale-handling)).

### Version

```lua
local major, minor, patch = RQ_GetVersion()
```

Returns the Reliquary version as three integers.

### Typed lookups

Each supported DBC has a dedicated function that pre-selects the DBC name.
All return `row, err` with the same semantics as `RQ_GetRow`.

**Spell system**

| Function | DBC |
|---|---|
| `RQ_GetSpell(id)` | `Spell` |
| `RQ_GetSpellCastTimes(id)` | `SpellCastTimes` |
| `RQ_GetSpellCategory(id)` | `SpellCategory` |
| `RQ_GetSpellDispelType(id)` | `SpellDispelType` |
| `RQ_GetSpellDuration(id)` | `SpellDuration` |
| `RQ_GetSpellIcon(id)` | `SpellIcon` |
| `RQ_GetSpellItemEnchantment(id)` | `SpellItemEnchantment` |
| `RQ_GetSpellMechanic(id)` | `SpellMechanic` |
| `RQ_GetSpellRadius(id)` | `SpellRadius` |
| `RQ_GetSpellRange(id)` | `SpellRange` |
| `RQ_GetSpellShapeshiftForm(id)` | `SpellShapeshiftForm` |

**Items**

| Function | DBC |
|---|---|
| `RQ_GetItemBagFamily(id)` | `ItemBagFamily` |
| `RQ_GetItemClass(id)` | `ItemClass` |
| `RQ_GetItemDisplayInfo(id)` | `ItemDisplayInfo` |
| `RQ_GetItemRandomProperties(id)` | `ItemRandomProperties` |
| `RQ_GetItemSet(id)` | `ItemSet` |
| `RQ_GetItemSubClass(classId, subclassId)` | `ItemSubClass` |

**Characters, classes, races**

| Function | DBC |
|---|---|
| `RQ_GetCharStartOutfit(id)` | `CharStartOutfit` |
| `RQ_GetChrClasses(id)` | `ChrClasses` |
| `RQ_GetChrRaces(id)` | `ChrRaces` |
| `RQ_GetTalent(id)` | `Talent` |
| `RQ_GetTalentTab(id)` | `TalentTab` |

**World and map**

| Function | DBC |
|---|---|
| `RQ_GetAreaTable(id)` | `AreaTable` |
| `RQ_GetAreaTrigger(id)` | `AreaTrigger` |
| `RQ_GetMap(id)` | `Map` |
| `RQ_GetTaxiNodes(id)` | `TaxiNodes` |
| `RQ_GetTaxiPath(id)` | `TaxiPath` |
| `RQ_GetTaxiPathNode(id)` | `TaxiPathNode` |
| `RQ_GetWorldMapArea(id)` | `WorldMapArea` |
| `RQ_GetWorldSafeLocs(id)` | `WorldSafeLocs` |

**Creatures and factions**

| Function | DBC |
|---|---|
| `RQ_GetCreatureFamily(id)` | `CreatureFamily` |
| `RQ_GetCreatureType(id)` | `CreatureType` |
| `RQ_GetFaction(id)` | `Faction` |
| `RQ_GetFactionTemplate(id)` | `FactionTemplate` |

**Skills**

| Function | DBC |
|---|---|
| `RQ_GetSkillLine(id)` | `SkillLine` |
| `RQ_GetSkillLineAbility(id)` | `SkillLineAbility` |
| `RQ_GetSkillLineCategory(id)` | `SkillLineCategory` |

**Miscellaneous**

| Function | DBC |
|---|---|
| `RQ_GetLFGDungeons(id)` | `LFGDungeons` |
| `RQ_GetLock(id)` | `Lock` |
| `RQ_GetLockType(id)` | `LockType` |
| `RQ_GetMailTemplate(id)` | `MailTemplate` |
| `RQ_GetQuestInfo(id)` | `QuestInfo` |
| `RQ_GetQuestSort(id)` | `QuestSort` |

### Row format

A successful lookup returns a Lua table with both named and positional keys:

```lua
local row = RQ_GetSpell(133)
-- Named:      row.id, row.name, row.school, row.category, ...
-- Positional: row[1], row[2],   row[3],     row[4],       ...
```

Positional keys follow column order as defined in the DBC schema and skip
locale variants that were filtered out (see below). Named and positional
access refer to the same values.

### Locale handling

Localized string fields are named `<field>_<locale>` in the raw DBC schema
(e.g. `name_enUS`, `name_frFR`). The optional `locale` argument to `RQ_GetRow`
controls how these are presented:

| Argument | Behavior |
|---|---|
| omitted or `nil` | The `enUS` variant is returned as `name` (suffix stripped); all other locale variants are excluded |
| `"enUS"`, `"frFR"`, etc. | The specified locale's variant is returned as `name`; all others excluded |
| `"all"` | All 8 variants are included with full names (`name_enUS`, `name_koKR`, …) |

Typed lookup functions always use the `enUS` default. Use `RQ_GetRow` with an
explicit locale if you need another language.

Supported locales: `enUS`, `koKR`, `frFR`, `deDE`, `zhCN`, `ruRU`, `esES`, `ptPT`.

### Error handling

Bad argument types or counts raise a hard Lua error with a usage string.
All other failures return `nil` plus a descriptive error string:

| Situation | Error string |
|---|---|
| DBC name not recognised | `RQ_GetRow: unknown DBC '<name>'` |
| ID not present in DBC | `RQ_GetRow: no record with id <id> in '<name>'` |
| DBC file absent from all MPQs | `RQ_GetRow: 'DBFilesClient\<name>.dbc' not found in any MPQ` |
| DBC file malformed | `RQ_GetRow: failed to parse '<name>.dbc': <reason>` |

Typed lookup functions produce equivalent messages prefixed with their own
function name.

### Notes

**Lua 5.0 compatibility.** WoW 1.12.1 uses Lua 5.0, which differs from later
versions in a few ways that affect Reliquary usage:

- String methods cannot be called with `:` syntax (`err:find(...)` fails).
  Use the module-level functions instead: `string.find(err, "...")`.
- `math.huge` is not defined. Use large numeric literals such as `2^53` when
  you need values outside the normal range.
- `lua_isstring` returns true for numbers, so passing a number where a string
  is expected (e.g. a DBC name) will not trigger a usage error; the number
  will be coerced to a string and treated as an unknown DBC name.
- Similarly, `lua_isnumber` returns true for numeric strings, so `"133"`
  is accepted wherever a numeric ID is expected.

**Lazy loading.** Each DBC is parsed on first access and cached for the
remainder of the session. The first call to any function that touches a
given DBC may be slightly slower than subsequent calls.

**Logging.** Reliquary writes warnings and errors to `Logs\Reliquary.log`.
Normal successful lookups produce no log output.

## Examples

### Detecting Reliquary

Check for `RQ_GetVersion` before calling any `RQ_*` function. This lets your
addon degrade gracefully when Reliquary is not installed.

```lua
if not RQ_GetVersion then
    DEFAULT_CHAT_FRAME:AddMessage("MyAddon requires Reliquary.")
    return
end

local major, minor, patch = RQ_GetVersion()
-- e.g. 0, 1, 0
```

### Basic spell lookup

```lua
local row, err = RQ_GetSpell(133)
if not row then
    DEFAULT_CHAT_FRAME:AddMessage("Reliquary: " .. err)
    return
end

-- Named field access
DEFAULT_CHAT_FRAME:AddMessage("Spell: " .. row.name)  -- "Fireball"
DEFAULT_CHAT_FRAME:AddMessage("School: " .. row.school)

-- Positional access (column order)
local id, school, category = row[1], row[2], row[3]
```

### Locale-aware lookup

```lua
-- French name for Fireball
local row = RQ_GetRow("Spell", 133, "frFR")
if row then
    DEFAULT_CHAT_FRAME:AddMessage(row.name)  -- French localization
end

-- All locales at once
local row = RQ_GetRow("Spell", 133, "all")
if row then
    DEFAULT_CHAT_FRAME:AddMessage(row.name_enUS)
    DEFAULT_CHAT_FRAME:AddMessage(row.name_deDE)
end
```

### ItemSubClass lookup

`RQ_GetItemSubClass` takes two arguments instead of one. The composite key is
`(classId, subclassId)`:

```lua
-- Weapon (class 2), One-Handed Axes (subclass 0)
local row, err = RQ_GetItemSubClass(2, 0)
if not row then
    DEFAULT_CHAT_FRAME:AddMessage("Reliquary: " .. err)
    return
end
DEFAULT_CHAT_FRAME:AddMessage(row.name)
```

### Generic lookup for dynamic DBC access

Use `RQ_GetRow` when the DBC name is not known at write time:

```lua
local function LookupDbc(dbcName, id)
    local row, err = RQ_GetRow(dbcName, id)
    if not row then
        DEFAULT_CHAT_FRAME:AddMessage("Lookup failed: " .. err)
        return nil
    end
    return row
end

local areaRow = LookupDbc("AreaTable", 1519)  -- Stormwind City
local mapRow  = LookupDbc("Map", 0)           -- Eastern Kingdoms
```

## Installation

1. Build `Reliquary.dll` (see below) or obtain a pre-built binary.
2. Copy `Reliquary.dll` to your TurtleWoW plugin directory alongside
   `WoW.exe`.
3. The plugin loader calls `Load()` on startup, which activates the Lua
   function hooks.

## Building on Linux (cross-compile)

### Prerequisites

1. Install the Rust toolchain:
   ```
   https://rustup.rs
   ```

2. Add the 32-bit Windows target:
   ```bash
   rustup target add i686-pc-windows-msvc
   ```

3. Install `clang-cl` (C compiler) and `lld-link` (linker):
   ```bash
   # Arch / CachyOS
   sudo pacman -S clang lld

   # Debian / Ubuntu
   sudo apt install clang lld
   ```

4. Install `xwin` to obtain the MSVC sysroot:
   ```bash
   cargo install xwin
   ```

5. Populate the sysroot with the 32-bit (x86) libraries. Run from the
   repository root:
   ```bash
   xwin --accept-license --arch x86 splat --include-debug-libs --output xwinSDK
   ```
   If `xwin` is not in your `PATH`:
   ```bash
   ~/.local/share/cargo/bin/xwin --accept-license --arch x86 splat --include-debug-libs --output xwinSDK
   ```
   The `--arch x86` flag is required; the default download is x86_64 only.

### Build

Use `make` rather than invoking `cargo` directly. The Makefile sets the
required `CC` and `CFLAGS` environment variables with absolute paths so
that the C compiler can locate the xwinSDK headers regardless of its
working directory.

```bash
make          # release build (default)
make debug    # debug build
```

Output: `target/i686-pc-windows-msvc/release/Reliquary.dll`

## Building on Windows

Install the Rust toolchain and add the 32-bit Windows target:

```cmd
rustup target add i686-pc-windows-msvc
```

With a 32-bit MSVC toolchain available (Visual Studio or the standalone
Build Tools), no further configuration is needed:

```cmd
cargo build --release --target i686-pc-windows-msvc
```
