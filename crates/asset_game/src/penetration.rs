use weapon_iw4::{PenetrationDepthTable, split_pen_key};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenTableError {
    MissingMagic,
    OddPair,
    BadNumber,
    UnknownKey,
    NotUtf8,
}

pub fn parse_pen_table_info_string(text: &str) -> Result<PenetrationDepthTable, PenTableError> {
    let text = text.trim();
    let rest = text
        .strip_prefix("BULLET_PEN_TABLE")
        .ok_or(PenTableError::MissingMagic)?;
    let mut table = PenetrationDepthTable::empty();
    let mut parts = rest.split('\\').filter(|p| !p.is_empty());
    loop {
        let Some(key) = parts.next() else {
            break;
        };
        let Some(value) = parts.next() else {
            return Err(PenTableError::OddPair);
        };
        let parsed: f32 = value.parse().map_err(|_| PenTableError::BadNumber)?;
        let (ptype, surf) = split_pen_key(key).ok_or(PenTableError::UnknownKey)?;
        table.set_depth(ptype, surf, parsed);
    }
    Ok(table)
}

pub fn parse_pen_table_rawfile(data: &[u8]) -> Result<PenetrationDepthTable, PenTableError> {
    let text = core::str::from_utf8(data).map_err(|_| PenTableError::NotUtf8)?;
    parse_pen_table_info_string(text)
}

pub fn capture_pen_table(
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Option<PenetrationDepthTable> {
    // The multiplayer table, or BO1 zombies' single-player one (in code_post_gfx).
    if name != "info/bullet_penetration_mp" && name != "info/bullet_penetration_sp" {
        return None;
    }
    let mut bytes = if zlib_compressed {
        asset_transport::inflate_zlib(data).ok()?
    } else {
        data.to_vec()
    };
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    parse_pen_table_rawfile(&bytes).ok()
}

#[derive(Default)]
struct PenTableSink {
    table: Option<PenetrationDepthTable>,
}

impl fastfile_t5::AssetLinkSink for PenTableSink {
    fn loaded(
        &mut self,
        _s: &fastfile_t5::ZoneStream<'_>,
        _ty: fastfile_t5::AssetType,
        _slot: fastfile_t5::Ptr,
        _insert_slot: Option<fastfile_t5::Ptr>,
    ) -> fastfile_t5::Result<()> {
        Ok(())
    }

    fn alias(
        &mut self,
        _ty: fastfile_t5::AssetType,
        _slot: fastfile_t5::Ptr,
        _target: fastfile_t5::Ptr,
    ) -> fastfile_t5::Result<()> {
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_t5::Result<()> {
        if let Some(table) = capture_pen_table(name, data, zlib_compressed) {
            self.table = Some(table);
        }
        Ok(())
    }
}

impl fastfile_t5::AssetSink for PenTableSink {
    fn set_script_strings(&mut self, _strings: fastfile_t5::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut fastfile_t5::ZoneStream<'_>,
        _index: usize,
        ty: fastfile_t5::AssetType,
        slot: fastfile_t5::Ptr,
    ) -> fastfile_t5::Result<()> {
        fastfile_t5::load_asset_at_observed(s, ty, slot, self).map(|_| ())
    }
}

/// BO1's bullet penetration depths (`info/bullet_penetration_sp`), which its
/// `code_post_gfx` zone carries: how deep each kind of bullet goes into each surface.
pub fn load_t5_pen_table(zone: &std::path::Path) -> Option<PenetrationDepthTable> {
    let image = asset_transport::open_zone(zone).ok()?;
    let header = image.t5_header().ok()?;
    let mut memory = asset_transport::T5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).ok()?;
    let mut sink = PenTableSink::default();
    let _ = fastfile_t5::load_zone(&mut stream, &mut sink);
    sink.table
}
