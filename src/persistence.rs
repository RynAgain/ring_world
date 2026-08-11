/// Save/load persistence: a versioned little-endian binary format with no
/// external dependencies (no serde). The save is intentionally tiny: instead
/// of whole chunks we persist only the player's EDIT OVERLAY (see
/// `ChunkManager::edits`) plus player state and the day/night phase. Terrain
/// is deterministic from the seed, so regenerating a chunk and re-applying
/// its recorded edits reproduces the world exactly.
///
/// Layout (all little-endian):
///   magic     8 bytes  b"RINGSAV\x01" (format version baked into last byte)
///   player    3 x f64  (theta, y, height)  -- body center
///   spawn     3 x f64  respawn point
///   health    f32
///   hotbar    u8
///   creative  u8 (0 or 1)
///   phase     f64      shadow-square orbital phase
///   inv_len   u32, then per slot:
///             u8 present; if 1: u8 item_type, u32 count
///   n_chunks  u32, then per chunk:
///             u32 ring_index, u32 width_index, u32 height_index,
///             u32 n_edits, then per edit: u16 local_index, u8 voxel_type

use std::path::Path;

pub const SAVE_PATH: &str = "world.save";
const MAGIC: [u8; 8] = *b"RINGSAV\x01";

#[derive(Debug, Clone, PartialEq)]
pub struct WorldSave {
    /// Player body-center ring position as (theta, y, height).
    pub player_position: (f64, f64, f64),
    /// Respawn point as (theta, y, height).
    pub spawn_position: (f64, f64, f64),
    pub health: f32,
    pub hotbar_index: u8,
    pub creative_mode: bool,
    /// ShadowSquares orbital phase (radians).
    pub shadow_phase: f64,
    /// One entry per inventory slot: None = empty, Some((type as u8, count)).
    pub inventory: Vec<Option<(u8, u32)>>,
    /// Per-chunk edit overlay: ((ring, width, height), [(local_index, type)]).
    pub edits: Vec<((u32, u32, u32), Vec<(u16, u8)>)>,
}

impl WorldSave {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(&MAGIC);
        for &v in &[
            self.player_position.0,
            self.player_position.1,
            self.player_position.2,
            self.spawn_position.0,
            self.spawn_position.1,
            self.spawn_position.2,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.health.to_le_bytes());
        out.push(self.hotbar_index);
        out.push(self.creative_mode as u8);
        out.extend_from_slice(&self.shadow_phase.to_le_bytes());

        out.extend_from_slice(&(self.inventory.len() as u32).to_le_bytes());
        for slot in &self.inventory {
            match slot {
                Some((item, count)) => {
                    out.push(1);
                    out.push(*item);
                    out.extend_from_slice(&count.to_le_bytes());
                }
                None => out.push(0),
            }
        }

        out.extend_from_slice(&(self.edits.len() as u32).to_le_bytes());
        for ((ring, width, height), list) in &self.edits {
            out.extend_from_slice(&ring.to_le_bytes());
            out.extend_from_slice(&width.to_le_bytes());
            out.extend_from_slice(&height.to_le_bytes());
            out.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for (idx, vt) in list {
                out.extend_from_slice(&idx.to_le_bytes());
                out.push(*vt);
            }
        }
        out
    }

    /// Decode a save file. Returns None on any structural problem (bad magic,
    /// truncation) rather than panicking, so a corrupt file just means a
    /// fresh world instead of a crash.
    pub fn decode(bytes: &[u8]) -> Option<WorldSave> {
        let mut r = Reader { buf: bytes, pos: 0 };
        if r.take(8)? != MAGIC {
            return None;
        }
        let player_position = (r.f64()?, r.f64()?, r.f64()?);
        let spawn_position = (r.f64()?, r.f64()?, r.f64()?);
        let health = r.f32()?;
        let hotbar_index = r.u8()?;
        let creative_mode = r.u8()? != 0;
        let shadow_phase = r.f64()?;

        let inv_len = r.u32()? as usize;
        // Sanity cap: no realistic inventory exceeds this.
        if inv_len > 1024 {
            return None;
        }
        let mut inventory = Vec::with_capacity(inv_len);
        for _ in 0..inv_len {
            let present = r.u8()?;
            if present == 1 {
                inventory.push(Some((r.u8()?, r.u32()?)));
            } else {
                inventory.push(None);
            }
        }

        let n_chunks = r.u32()? as usize;
        let mut edits = Vec::with_capacity(n_chunks.min(4096));
        for _ in 0..n_chunks {
            let coord = (r.u32()?, r.u32()?, r.u32()?);
            let n = r.u32()? as usize;
            // 16^3 voxels per chunk is the hard upper bound on distinct edits.
            if n > 4096 {
                return None;
            }
            let mut list = Vec::with_capacity(n);
            for _ in 0..n {
                list.push((r.u16()?, r.u8()?));
            }
            edits.push((coord, list));
        }

        Some(WorldSave {
            player_position,
            spawn_position,
            health,
            hotbar_index,
            creative_mode,
            shadow_phase,
            inventory,
            edits,
        })
    }

    /// Write atomically: write a temp file, then swap it into place, so a
    /// crash mid-write never leaves a truncated save behind.
    pub fn write_to<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let path = path.as_ref();
        let tmp = path.with_extension("save.tmp");
        std::fs::write(&tmp, self.encode())?;
        // std::fs::rename fails on Windows if the target exists; remove first.
        let _ = std::fs::remove_file(path);
        std::fs::rename(&tmp, path)
    }

    pub fn read_from<P: AsRef<Path>>(path: P) -> Option<WorldSave> {
        let bytes = std::fs::read(path).ok()?;
        Self::decode(&bytes)
    }
}

/// Bounds-checked little-endian cursor over a byte slice.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        if end > self.buf.len() {
            return None;
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WorldSave {
        WorldSave {
            player_position: (1.25, -3.5, 12.0),
            spawn_position: (0.5, 2.0, 40.0),
            health: 17.5,
            hotbar_index: 4,
            creative_mode: true,
            shadow_phase: 2.71828,
            inventory: vec![
                Some((1, 64)),
                None,
                Some((29, 1)),
                None,
            ],
            edits: vec![
                ((3, 1, 2), vec![(0, 1), (4095, 26)]),
                ((255, 0, 0), vec![(100, 0)]),
            ],
        }
    }

    #[test]
    fn encode_decode_round_trips() {
        let save = sample();
        let decoded = WorldSave::decode(&save.encode()).expect("decode failed");
        assert_eq!(decoded, save);
    }

    #[test]
    fn empty_collections_round_trip() {
        let save = WorldSave {
            inventory: vec![],
            edits: vec![],
            ..sample()
        };
        let decoded = WorldSave::decode(&save.encode()).expect("decode failed");
        assert_eq!(decoded, save);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = sample().encode();
        bytes[0] = b'X';
        assert!(WorldSave::decode(&bytes).is_none());
    }

    #[test]
    fn truncation_is_rejected_at_every_length() {
        let bytes = sample().encode();
        for len in 0..bytes.len() {
            assert!(
                WorldSave::decode(&bytes[..len]).is_none(),
                "truncated save of {} bytes decoded",
                len
            );
        }
    }

    #[test]
    fn write_and_read_file_round_trips() {
        let save = sample();
        let path = std::env::temp_dir().join("ring_world_persistence_test.save");
        save.write_to(&path).expect("write failed");
        let loaded = WorldSave::read_from(&path).expect("read failed");
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded, save);
    }
}
