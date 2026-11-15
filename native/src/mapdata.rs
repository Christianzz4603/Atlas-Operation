//! Loader for the `.atlasmap.z` files produced by `tools/bake.py` (format v2).
use miniz_oxide::inflate::decompress_to_vec_zlib;

pub struct Country {
    pub name: String,
    pub rgb: [u8; 3],
    /// population in millions
    pub pop: f32,
    /// GDP in $B
    pub gdp: f32,
    /// standing military in thousands
    pub military: f32,
    /// base mobilizable manpower (pop * 12000 * militarization modifier)
    pub mobilizable: f32,
}

pub struct Capital {
    pub country: u16,
    pub x: u16,
    pub y: u16,
}

pub struct MapData {
    pub w: usize,
    pub h: usize,
    /// lon_min, lon_max, lat_min, lat_max
    pub crop: [f32; 4],
    /// index 0 is the "unowned" placeholder, real countries start at 1
    pub countries: Vec<Country>,
    pub capitals: Vec<Capital>,
    pub land: Vec<u8>,
    pub owner: Vec<u16>,
    pub outline: Vec<u8>,
    pub inner: Vec<u8>,
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.p.checked_add(n).ok_or_else(|| "length overflow".to_string())?;
        if end > self.d.len() {
            return Err("unexpected end of map data".to_string());
        }
        let s = &self.d[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

impl MapData {
    pub fn load(z: &[u8]) -> Result<MapData, String> {
        let raw = decompress_to_vec_zlib(z).map_err(|e| format!("inflate failed: {:?}", e))?;
        let mut r = Reader { d: &raw, p: 0 };
        if r.take(4)? != &b"ATLM"[..] {
            return Err("bad magic".to_string());
        }
        let version = r.u16()?;
        if version != 2 {
            return Err(format!("map format v{} not supported (re-run tools/bake.py)", version));
        }
        let w = r.u16()? as usize;
        let h = r.u16()? as usize;
        let n_countries = r.u16()? as usize;
        let n_caps = r.u16()? as usize;
        let crop = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];

        let mut countries = Vec::with_capacity(n_countries + 1);
        countries.push(Country { name: String::new(), rgb: [58, 111, 143], pop: 0.0, gdp: 0.0, military: 0.0, mobilizable: 1.0 });
        for _ in 0..n_countries {
            let len = r.u8()? as usize;
            let name = String::from_utf8_lossy(r.take(len)?).into_owned();
            let c = r.take(3)?;
            let rgb = [c[0], c[1], c[2]];
            let (pop, gdp, military, mobilizable) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            countries.push(Country { name, rgb, pop, gdp, military, mobilizable });
        }
        let mut capitals = Vec::with_capacity(n_caps);
        for _ in 0..n_caps {
            capitals.push(Capital { country: r.u16()?, x: r.u16()?, y: r.u16()? });
        }
        let n = w * h;
        let land = r.take(n)?.to_vec();
        let owner = r
            .take(n * 2)?
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let outline = r.take(n)?.to_vec();
        let inner = r.take(n)?.to_vec();
        Ok(MapData { w, h, crop, countries, capitals, land, owner, outline, inner })
    }
}
