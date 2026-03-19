use super::*;

pub fn parse_ascii_map(source: &str, ascii_map: &str) -> Result<ArenaMapDefinition, ContentError> {
    let rows = collect_map_rows(source, ascii_map)?;
    let (width_tiles, height_tiles, width_units, height_units) =
        validate_map_dimensions(source, &rows)?;
    let (team_a_anchor, team_b_anchor, obstacles) =
        parse_map_layout(source, &rows, width_tiles, height_tiles)?;

    Ok(ArenaMapDefinition {
        map_id: map_identifier(source),
        width_tiles,
        height_tiles,
        tile_units: DEFAULT_TILE_UNITS,
        width_units,
        height_units,
        team_a_anchor,
        team_b_anchor,
        obstacles,
    })
}

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
) -> Result<(u16, u16, u16, u16), ContentError> {
    let width = rows[0].chars().count();
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
            .checked_mul(DEFAULT_TILE_UNITS)
            .ok_or_else(|| ContentError::Validation {
                source: String::from(source),
                message: String::from("map width in world units overflowed u16"),
            })?;
    let height_units = height_tiles
        .checked_mul(DEFAULT_TILE_UNITS)
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
) -> Result<(AnchorPoint, AnchorPoint, Vec<ArenaMapObstacle>), ContentError> {
    let mut team_a_anchor = None;
    let mut team_b_anchor = None;
    let mut obstacles = Vec::new();
    let expected_width = usize::from(width_tiles);

    for (row_index, row) in rows.iter().enumerate() {
        validate_map_row_width(source, row, row_index, expected_width)?;

        for (column_index, glyph) in row.chars().enumerate() {
            let (center_x, center_y) = map_cell_center(
                width_tiles,
                height_tiles,
                DEFAULT_TILE_UNITS,
                column_index,
                row_index,
            )?;
            parse_map_glyph(
                source,
                glyph,
                row_index,
                column_index,
                center_x,
                center_y,
                &mut team_a_anchor,
                &mut team_b_anchor,
                &mut obstacles,
            )?;
        }
    }

    let team_a_anchor = team_a_anchor.ok_or_else(|| ContentError::Validation {
        source: String::from(source),
        message: String::from("map must contain one Team A anchor 'A'"),
    })?;
    let team_b_anchor = team_b_anchor.ok_or_else(|| ContentError::Validation {
        source: String::from(source),
        message: String::from("map must contain one Team B anchor 'B'"),
    })?;
    Ok((team_a_anchor, team_b_anchor, obstacles))
}

fn validate_map_row_width(
    source: &str,
    row: &str,
    row_index: usize,
    expected_width: usize,
) -> Result<(), ContentError> {
    let row_width = row.chars().count();
    if row_width != expected_width {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "row {} has width {} but expected {}",
                row_index + 1,
                row_width,
                expected_width
            ),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_map_glyph(
    source: &str,
    glyph: char,
    row_index: usize,
    column_index: usize,
    center_x: i16,
    center_y: i16,
    team_a_anchor: &mut Option<AnchorPoint>,
    team_b_anchor: &mut Option<AnchorPoint>,
    obstacles: &mut Vec<ArenaMapObstacle>,
) -> Result<(), ContentError> {
    match glyph {
        '.' | ' ' => Ok(()),
        'A' => set_team_anchor(source, "A", team_a_anchor, center_x, center_y),
        'B' => set_team_anchor(source, "B", team_b_anchor, center_x, center_y),
        '#' => {
            obstacles.push(map_obstacle(
                ArenaMapObstacleKind::Pillar,
                center_x,
                center_y,
            ));
            Ok(())
        }
        '+' => {
            obstacles.push(map_obstacle(
                ArenaMapObstacleKind::Shrub,
                center_x,
                center_y,
            ));
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

fn set_team_anchor(
    source: &str,
    label: &str,
    anchor: &mut Option<AnchorPoint>,
    center_x: i16,
    center_y: i16,
) -> Result<(), ContentError> {
    if anchor.replace((center_x, center_y)).is_some() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("map must contain exactly one Team {label} anchor"),
        });
    }
    Ok(())
}

fn map_obstacle(kind: ArenaMapObstacleKind, center_x: i16, center_y: i16) -> ArenaMapObstacle {
    ArenaMapObstacle {
        kind,
        center_x,
        center_y,
        half_width: DEFAULT_TILE_UNITS / 2,
        half_height: DEFAULT_TILE_UNITS / 2,
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
