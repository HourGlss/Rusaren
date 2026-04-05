use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::{
    AnchorPoint, ArenaMapDefinition, ArenaMapFeature, ArenaMapFeatureKind, ArenaMapObstacle,
    ArenaMapObstacleKind, ContentError, MAX_MAP_DIMENSION_TILES,
};

pub fn parse_ascii_map(
    source: &str,
    ascii_map: &str,
    tile_units: u16,
) -> Result<ArenaMapDefinition, ContentError> {
    let rows = collect_map_rows(source, ascii_map)?;
    let (width_tiles, height_tiles, width_units, height_units) =
        validate_map_dimensions(source, &rows, tile_units)?;
    let (footprint_mask, objective_mask, team_a_anchors, team_b_anchors, obstacles, features) =
        parse_map_layout(source, &rows, width_tiles, height_tiles, tile_units)?;

    Ok(ArenaMapDefinition {
        map_id: map_identifier(source),
        width_tiles,
        height_tiles,
        tile_units,
        width_units,
        height_units,
        objective_target_ms: 0,
        footprint_mask,
        objective_mask,
        team_a_anchors,
        team_b_anchors,
        obstacles,
        features,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapRegistryYaml {
    maps: Vec<MapObjectiveSettingsYaml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapObjectiveSettingsYaml {
    id: String,
    objective_target_ms: u32,
}

pub fn parse_map_registry_yaml(
    source: &str,
    yaml: &str,
) -> Result<BTreeMap<String, u32>, ContentError> {
    let parsed: MapRegistryYaml =
        serde_yaml::from_str(yaml).map_err(|error| ContentError::Parse {
            source: String::from(source),
            message: error.to_string(),
        })?;
    if parsed.maps.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map registry must contain at least one map entry"),
        });
    }

    let mut settings = BTreeMap::new();
    for entry in parsed.maps {
        let map_id = entry.id.trim();
        if map_id.is_empty() {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: String::from("map registry entries must have a non-empty id"),
            });
        }
        if entry.objective_target_ms == 0 {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: format!("map '{map_id}' objective_target_ms must be greater than zero"),
            });
        }
        if settings
            .insert(String::from(map_id), entry.objective_target_ms)
            .is_some()
        {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: format!("map registry contains duplicate entry '{map_id}'"),
            });
        }
    }

    Ok(settings)
}

type ParsedMapLayout = (
    Vec<u8>,
    Vec<u8>,
    Vec<AnchorPoint>,
    Vec<AnchorPoint>,
    Vec<ArenaMapObstacle>,
    Vec<ArenaMapFeature>,
);

fn collect_map_rows<'a>(source: &str, ascii_map: &'a str) -> Result<Vec<&'a str>, ContentError> {
    let rows = ascii_map
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map must contain at least one non-empty row"),
        });
    }
    Ok(rows)
}

fn validate_map_dimensions(
    source: &str,
    rows: &[&str],
    tile_units: u16,
) -> Result<(u16, u16, u16, u16), ContentError> {
    if tile_units == 0 {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map tile_units must be greater than zero"),
        });
    }
    let width = rows
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0);
    if width == 0 || width > MAX_MAP_DIMENSION_TILES {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "map width {width} is outside the supported range 1..={MAX_MAP_DIMENSION_TILES}"
            ),
        });
    }
    if rows.len() > MAX_MAP_DIMENSION_TILES {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "map height {} is outside the supported range 1..={MAX_MAP_DIMENSION_TILES}",
                rows.len()
            ),
        });
    }

    let width_tiles = u16::try_from(width).map_err(|_| ContentError::Validation {
        source: String::from(source),
        message: format!("map width {width} does not fit into u16"),
    })?;
    let height_tiles = u16::try_from(rows.len()).map_err(|_| ContentError::Validation {
        source: String::from(source),
        message: format!("map height {} does not fit into u16", rows.len()),
    })?;
    let width_units =
        width_tiles
            .checked_mul(tile_units)
            .ok_or_else(|| ContentError::Validation {
                source: String::from(source),
                message: String::from("map width in world units overflowed u16"),
            })?;
    let height_units =
        height_tiles
            .checked_mul(tile_units)
            .ok_or_else(|| ContentError::Validation {
                source: String::from(source),
                message: String::from("map height in world units overflowed u16"),
            })?;
    Ok((width_tiles, height_tiles, width_units, height_units))
}

fn parse_map_layout(
    source: &str,
    rows: &[&str],
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
) -> Result<ParsedMapLayout, ContentError> {
    let mut footprint_mask = blank_map_mask(width_tiles, height_tiles);
    let mut objective_mask = blank_map_mask(width_tiles, height_tiles);
    let mut team_a_anchors = Vec::new();
    let mut team_b_anchors = Vec::new();
    let mut obstacles = Vec::new();
    let mut features = Vec::new();

    for (row_index, row) in rows.iter().enumerate() {
        let glyphs = row.chars().collect::<Vec<_>>();
        for column_index in 0..usize::from(width_tiles) {
            let glyph = glyphs.get(column_index).copied().unwrap_or(' ');
            let (center_x, center_y) = map_cell_center(
                width_tiles,
                height_tiles,
                tile_units,
                column_index,
                row_index,
            )?;
            parse_map_glyph(
                source,
                glyph,
                row_index,
                column_index,
                &mut footprint_mask,
                &mut objective_mask,
                width_tiles,
                tile_units,
                center_x,
                center_y,
                &mut team_a_anchors,
                &mut team_b_anchors,
                &mut obstacles,
                &mut features,
            )?;
        }
    }

    if team_a_anchors.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map must contain at least one Team A anchor 'A'"),
        });
    }
    if team_b_anchors.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map must contain at least one Team B anchor 'B'"),
        });
    }
    if team_a_anchors.len() > 3 {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map must contain at most three Team A anchors"),
        });
    }
    if team_b_anchors.len() > 3 {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map must contain at most three Team B anchors"),
        });
    }
    Ok((
        footprint_mask,
        objective_mask,
        team_a_anchors,
        team_b_anchors,
        obstacles,
        features,
    ))
}

fn blank_map_mask(width_tiles: u16, height_tiles: u16) -> Vec<u8> {
    let tile_count = usize::from(width_tiles) * usize::from(height_tiles);
    vec![0_u8; tile_count.div_ceil(8)]
}

fn set_map_mask_bit(mask: &mut [u8], width_tiles: u16, row_index: usize, column_index: usize) {
    let index = row_index * usize::from(width_tiles) + column_index;
    let byte_index = index / 8;
    let bit_index = index % 8;
    if let Some(byte) = mask.get_mut(byte_index) {
        *byte |= 1_u8 << bit_index;
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_map_glyph(
    source: &str,
    glyph: char,
    row_index: usize,
    column_index: usize,
    footprint_mask: &mut [u8],
    objective_mask: &mut [u8],
    width_tiles: u16,
    tile_units: u16,
    center_x: i16,
    center_y: i16,
    team_a_anchors: &mut Vec<AnchorPoint>,
    team_b_anchors: &mut Vec<AnchorPoint>,
    obstacles: &mut Vec<ArenaMapObstacle>,
    features: &mut Vec<ArenaMapFeature>,
) -> Result<(), ContentError> {
    match glyph {
        ' ' => Ok(()),
        '.' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            Ok(())
        }
        'A' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            team_a_anchors.push((center_x, center_y));
            Ok(())
        }
        'B' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            team_b_anchors.push((center_x, center_y));
            Ok(())
        }
        'X' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            set_map_mask_bit(objective_mask, width_tiles, row_index, column_index);
            Ok(())
        }
        '#' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            obstacles.push(map_obstacle(
                ArenaMapObstacleKind::Pillar,
                center_x,
                center_y,
                tile_units,
            ));
            Ok(())
        }
        '+' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            obstacles.push(map_obstacle(
                ArenaMapObstacleKind::Shrub,
                center_x,
                center_y,
                tile_units,
            ));
            Ok(())
        }
        'd' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            features.push(ArenaMapFeature {
                kind: ArenaMapFeatureKind::TrainingDummyResetFull,
                center_x,
                center_y,
            });
            Ok(())
        }
        'D' => {
            set_map_mask_bit(footprint_mask, width_tiles, row_index, column_index);
            features.push(ArenaMapFeature {
                kind: ArenaMapFeatureKind::TrainingDummyExecute,
                center_x,
                center_y,
            });
            Ok(())
        }
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "unsupported map glyph '{other}' at row {}, column {}",
                row_index + 1,
                column_index + 1
            ),
        }),
    }
}

fn map_obstacle(
    kind: ArenaMapObstacleKind,
    center_x: i16,
    center_y: i16,
    tile_units: u16,
) -> ArenaMapObstacle {
    ArenaMapObstacle {
        kind,
        center_x,
        center_y,
        half_width: tile_units / 2,
        half_height: tile_units / 2,
    }
}

fn map_identifier(source: &str) -> String {
    Path::new(source)
        .file_stem()
        .and_then(|value| value.to_str())
        .map_or_else(|| String::from("arena"), String::from)
}

fn map_cell_center(
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
    column: usize,
    row: usize,
) -> Result<(i16, i16), ContentError> {
    let width_units = i32::from(width_tiles) * i32::from(tile_units);
    let height_units = i32::from(height_tiles) * i32::from(tile_units);
    let origin_x = -width_units / 2;
    let origin_y = -height_units / 2;
    let center_x = origin_x
        + i32::try_from(column).unwrap_or(i32::MAX) * i32::from(tile_units)
        + i32::from(tile_units / 2);
    let center_y = origin_y
        + i32::try_from(row).unwrap_or(i32::MAX) * i32::from(tile_units)
        + i32::from(tile_units / 2);

    let x = i16::try_from(center_x).map_err(|_| ContentError::Validation {
        source: String::from("map"),
        message: format!("map column {column} overflowed i16 coordinates"),
    })?;
    let y = i16::try_from(center_y).map_err(|_| ContentError::Validation {
        source: String::from("map"),
        message: format!("map row {row} overflowed i16 coordinates"),
    })?;
    Ok((x, y))
}
