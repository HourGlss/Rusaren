# Maps Reference

This folder holds map-specific reference material that complements the higher-level notes in [10_maps.md](../10_maps.md).

Current contents:
- [_template.md](_template.md): authoring template for new ASCII maps, spawn anchors, blockers, shrubs, and training entities.

Runtime map authoring now distinguishes between:
- `server/content/maps/prototype_arena.txt`: fixed reference arena content
- `server/content/maps/template_arena.txt`: fixed spawn/objective template used by the live lobby map generator
- `server/content/maps/generated/`: inspection-only sample outputs created by `cargo run -p map_sample_builder -- --count 100`

Use this folder for documents that are specific to map authoring or individual arena layouts, rather than broad gameplay or simulation rules.
