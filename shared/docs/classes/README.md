# classes

This directory contains class-specific design notes that complement the live authored YAML data.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `bard.md`: Design note for the Bard class that complements the live authored content.
- `cleric.md`: Design note for the Cleric class that complements the live authored content.
- `druid.md`: Design note for the Druid class that complements the live authored content.
- `mage.md`: Design note for the Mage class that complements the live authored content.
- `necromancer.md`: Design note for the Necromancer class that complements the live authored content.
- `paladin.md`: Design note for the Paladin class that complements the live authored content.
- `ranger.md`: Design note for the Ranger class that complements the live authored content.
- `rogue.md`: Design note for the Rogue class that complements the live authored content.
- `warrior.md`: Design note for the Warrior class that complements the live authored content.

## Planned 0.9 Class Color Language
Player tokens are planned to communicate build identity without changing gameplay collision:
- skill slot `1` colors the player center
- skill slots `2` through `5` render outward rings in pick order
- unpicked future slots render as black rings
- the outer border is team-relative per client: friendly is dark blue, enemy is red
- positive statuses render as a thin halo on the right side of the token
- negative statuses render as a thin halo on the left side of the token
- multiple statuses on one side split into distinct stacked sections ordered by remaining duration, longest at the top and shortest at the bottom

## WoW-Mapped Class Colors
These classes use the familiar WoW raid-class palette.

| Class | Mapping | Color |
| --- | --- | --- |
| `warrior` | WoW Warrior | `#C79C6E` |
| `mage` | WoW Mage | `#69CCF0` |
| `rogue` | WoW Rogue | `#FFF569` |
| `paladin` | WoW Paladin | `#F58CBA` |
| `druid` | WoW Druid | `#FF7D0A` |
| `ranger` | WoW Hunter | `#ABD473` |
| `cleric` | WoW Priest | `#FFFFFF` |

## Non-WoW Distinct Palette Reserve
For non-WoW classes and future class growth, the project reserves a categorical palette derived from Glasbey et al., 2007, "Colour displays for categorical images." The list below is saved so future classes can consume distinct colors without re-choosing the palette each time.

Used now:
- `bard` uses `#8C3BFF`
- `necromancer` uses `#6B004F`

Reserved Glasbey-style palette entries:

| Index | Color | Status |
| --- | --- | --- |
| `1` | `#D60000` | reserved |
| `2` | `#8C3BFF` | used by `bard` |
| `3` | `#018700` | reserved |
| `4` | `#00ACC6` | reserved |
| `5` | `#97FF00` | reserved |
| `6` | `#FF7ED1` | reserved |
| `7` | `#6B004F` | used by `necromancer` |
| `8` | `#FFA52F` | reserved |
| `9` | `#573B00` | reserved |
| `10` | `#005659` | reserved |
| `11` | `#0000DD` | reserved |
| `12` | `#00FDCF` | reserved |
| `13` | `#A17569` | reserved |
| `14` | `#BCB6FF` | reserved |
| `15` | `#95B577` | reserved |
| `16` | `#BF03B8` | reserved |
| `17` | `#645474` | reserved |
| `18` | `#790000` | reserved |
| `19` | `#0774D8` | reserved |
| `20` | `#FDF490` | reserved |
| `21` | `#004B00` | reserved |
| `22` | `#8E7900` | reserved |
| `23` | `#FF7266` | reserved |
| `24` | `#EDB8B8` | reserved |
| `25` | `#5D7E66` | reserved |
| `26` | `#9AE4FF` | reserved |
| `27` | `#EB0077` | reserved |
| `28` | `#A57BB8` | reserved |
| `29` | `#5900A3` | reserved |
| `30` | `#03C600` | reserved |
| `31` | `#9E4B00` | reserved |
| `32` | `#9C3B4F` | reserved |
