//! Economy: stockpiles, buildings, resource regions, treasury regen and AI building.
//! Port of WarSim._regenTreasury / _buildingOutputPerYear / buildCost / upgradeCost /
//! canPlaceBuildingAt / placeBuilding / upgradeBuilding / dismantleBuilding, the _runAiEconomy
//! family and World.computeResourceMap.
//! Not ported yet: roads + road blockage, port, missile silo, air defense, ideology/policy
//! modifiers (all multipliers are 1).
use crate::mapdata::MapData;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Res {
    Money,
    Bricks,
    Food,
    Steel,
    Oil,
    Uranium,
}
pub const RES_N: usize = 6;
pub type Stock = [f32; RES_N];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Special {
    None,
    /// raises the national population ceiling
    City,
    /// adds population every year
    Village,
}

const BUILDING_SIZE: i32 = 4;
const OUTPUT_PER_LEVEL_MULT: f32 = 0.85;
const OUTPUT_SCALE_EXP: f32 = 0.7;
const OUTPUT_REF_POP: f32 = 20.0;
const FARM_OUTPUT_BASE_FRAC: f32 = 0.4;
const MAX_LEVEL: u8 = 3;
const CITY_MAX_POP_BONUS: f32 = 0.15;
const CITY_BONUS_HALF_LIFE_WEIGHT: f32 = 4.0;

// CONFIG.treasuryRegen / civilianFood / economyCaps
const GDP_ANNUAL_YIELD: f32 = 0.06;
const MIN_ANNUAL_INCOME: f32 = 12.0;
const PER_POP_ANNUAL_INCOME: f32 = 0.6;
const STARTING_TREASURY_MIN: f32 = 12.0;
const TERRITORY_FLOOR: f32 = 0.2;
const CAPITAL_FALLEN_MULT: f32 = 0.55;
const CIVILIAN_FOOD_PER_POP: f32 = 0.15;
const MATERIAL_BASE: f32 = 250.0;
const MATERIAL_PER_REF_POP: f32 = 40.0;
const FOOD_BASE: f32 = 90.0;
const FOOD_PER_REF_POP: f32 = 18.0;
const MONEY_MIN: f32 = 80.0;
// CONFIG.aiEconomy
const AI_INTERVAL_S: f32 = 3.6;
const AI_JITTER: f32 = 0.55;
const AI_MONEY_RESERVE: f32 = 0.1;
const AI_UPGRADE_BIAS: f32 = 0.5;
const AI_SKILL: f32 = 0.75;

pub struct Cat {
    pub kind: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub money: f32,
    pub need: &'static [(Res, f32)],
    /// (resource, per year); None for city / village (they act on population)
    pub produces: Option<(Res, f32)>,
    pub color: [u8; 3],
    /// index into the resource-deposit kinds if this building gets a bonus on matching deposits
    pub spot: Option<usize>,
    pub dismantle_s: f32,
    pub special: Special,
    /// must touch water (ports)
    pub coastal: bool,
    pub max_level: u8,
}

pub const IDX_FARM: usize = 0;
pub const IDX_CONSTRUCTION: usize = 1;
pub const IDX_METALFAB: usize = 2;
pub const IDX_BANK: usize = 5;
pub const IDX_CITY: usize = 6;
pub const IDX_VILLAGE: usize = 7;
pub const IDX_PORT: usize = 8;
pub const IDX_SILO: usize = 9;
pub const IDX_AIRDEF: usize = 10;
const PORT_WATER_REACH: i32 = 2;

pub static CATALOG: [Cat; 11] = [
    Cat { kind: "farm", name: "Farm", blurb: "Grows food that feeds your people and troops.", money: 9.0, need: &[], produces: Some((Res::Food, 2000.0)), color: [52, 168, 83], spot: Some(0), dismantle_s: 8.0, special: Special::None, coastal: false, max_level: 3 },
    Cat { kind: "construction", name: "Construction", blurb: "Turns food into bricks.", money: 6.0, need: &[(Res::Food, 40.0)], produces: Some((Res::Bricks, 240.0)), color: [210, 126, 42], spot: Some(1), dismantle_s: 14.0, special: Special::None, coastal: false, max_level: 3 },
    Cat { kind: "metalfab", name: "Metal Fabric", blurb: "Smelts bricks into steel.", money: 10.0, need: &[(Res::Bricks, 120.0)], produces: Some((Res::Steel, 127.5)), color: [90, 120, 168], spot: Some(2), dismantle_s: 16.0, special: Special::None, coastal: false, max_level: 3 },
    Cat { kind: "oil", name: "Oil Derrick", blurb: "Pumps oil.", money: 12.0, need: &[(Res::Bricks, 85.0), (Res::Steel, 65.0)], produces: Some((Res::Oil, 17.0)), color: [91, 66, 119], spot: Some(3), dismantle_s: 12.0, special: Special::None, coastal: false, max_level: 3 },
    Cat { kind: "uranium", name: "Uranium Mine", blurb: "Digs uranium.", money: 15.0, need: &[(Res::Bricks, 120.0), (Res::Steel, 90.0)], produces: Some((Res::Uranium, 0.85)), color: [160, 192, 48], spot: Some(4), dismantle_s: 15.0, special: Special::None, coastal: false, max_level: 3 },
    Cat { kind: "bank", name: "Bank", blurb: "Earns extra money.", money: 19.0, need: &[(Res::Bricks, 150.0), (Res::Steel, 130.0)], produces: Some((Res::Money, 5.1)), color: [240, 180, 41], spot: None, dismantle_s: 10.0, special: Special::None, coastal: false, max_level: 3 },
    Cat { kind: "city", name: "City", blurb: "Raises how many people your nation can hold.", money: 17.0, need: &[(Res::Bricks, 320.0), (Res::Food, 150.0)], produces: None, color: [200, 205, 214], spot: None, dismantle_s: 18.0, special: Special::City, coastal: false, max_level: 3 },
    Cat { kind: "village", name: "Village", blurb: "Slowly adds population every year.", money: 5.0, need: &[(Res::Bricks, 55.0), (Res::Food, 30.0)], produces: None, color: [168, 140, 104], spot: None, dismantle_s: 10.0, special: Special::Village, coastal: false, max_level: 3 },
    Cat { kind: "port", name: "Port", blurb: "Lets your troops invade by sea. Must be on the coast.", money: 11.0, need: &[(Res::Bricks, 90.0), (Res::Steel, 70.0)], produces: None, color: [70, 150, 200], spot: None, dismantle_s: 12.0, special: Special::None, coastal: true, max_level: 3 },
    Cat { kind: "missile-silo", name: "Missile Silo", blurb: "Launches long-range missiles. Needs oil and uranium per shot.", money: 30.0, need: &[(Res::Bricks, 170.0), (Res::Steel, 210.0)], produces: None, color: [190, 70, 70], spot: None, dismantle_s: 18.0, special: Special::None, coastal: false, max_level: 1 },
    Cat { kind: "air-defense", name: "Air Defense", blurb: "Shoots down incoming missiles over your land.", money: 18.0, need: &[(Res::Bricks, 130.0), (Res::Steel, 160.0)], produces: None, color: [60, 165, 175], spot: None, dismantle_s: 14.0, special: Special::None, coastal: false, max_level: 3 },
];

/// Village output in millions of people per year (level 1).
pub const VILLAGE_POP_PER_YEAR: f32 = 0.025;

/// (name, weight, output multiplier, overlay alpha)
pub const RARITY: [(&str, f32, f32, f32); 3] = [("common", 60.0, 1.15, 0.32), ("uncommon", 30.0, 1.4, 0.5), ("rare", 10.0, 1.9, 0.75)];
const RESOURCE_KINDS: usize = 5;
const REGION_CELLS: usize = 3200;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16777216.0
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() as usize) % n.max(1)
    }
}

#[derive(Clone)]
pub struct Building {
    pub id: u32,
    pub kind: usize,
    pub country: u16,
    pub gx: i32,
    pub gy: i32,
    pub level: u8,
    /// sim time at which a dismantle finishes
    pub dismantle_at: Option<f32>,
}

pub struct EconCtx<'a> {
    pub names: &'a [String],
    pub cells: &'a [u32],
    pub orig: &'a [u32],
    pub fallen: &'a [bool],
    pub eliminated: &'a [bool],
    pub pop: &'a [f32],
    pub gdp: &'a [f32],
    pub bbox: &'a [[i32; 4]],
    /// regime+policy multipliers per nation (ideology.rs)
    pub mod_money: &'a [f32],
    pub mod_prod: &'a [f32],
}

pub struct Econ {
    w: usize,
    h: usize,
    pub stock: Vec<Stock>,
    pub buildings: Vec<Building>,
    occ: Vec<u32>,
    resource_grid: Vec<u8>,
    next_id: u32,
    ai_next: Vec<f32>,
    rng: Rng,
    decision_mult: f32,
    cap_div: f32,
    cap_max: usize,
    own_timer: f32,
}

pub fn farm_scale(pop: f32) -> f32 {
    let live = pop.max(0.0);
    let effective = if live > OUTPUT_REF_POP { OUTPUT_REF_POP * (live / OUTPUT_REF_POP).powf(OUTPUT_SCALE_EXP) } else { live };
    FARM_OUTPUT_BASE_FRAC + (1.0 - FARM_OUTPUT_BASE_FRAC) * effective / OUTPUT_REF_POP
}

fn compute_resource_map(land: &[u8], w: usize, h: usize, rng: &mut Rng) -> Vec<u8> {
    let n = w * h;
    let mut out = vec![0u8; n];
    let mut land_cells: Vec<usize> = (0..n).filter(|&i| land[i] == 1).collect();
    if land_cells.is_empty() {
        return out;
    }
    let region_count = land_cells.len().min(RESOURCE_KINDS.max((land_cells.len() as f32 / REGION_CELLS as f32).round() as usize));
    for i in (1..land_cells.len()).rev() {
        let j = rng.range(i + 1);
        land_cells.swap(i, j);
    }
    let mut owner = vec![-1i32; n];
    let mut fr: Vec<usize> = Vec::with_capacity(land_cells.len());
    for (s, &c) in land_cells.iter().take(region_count).enumerate() {
        owner[c] = s as i32;
        fr.push(c);
    }
    let mut head = 0usize;
    while head < fr.len() {
        let sw = head + rng.range(fr.len() - head);
        fr.swap(sw, head);
        let i = fr[head];
        head += 1;
        let r = owner[i];
        let (x, y) = ((i % w) as i32, (i / w) as i32);
        let mut d = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)];
        for k in (1..d.len()).rev() {
            let j = rng.range(k + 1);
            d.swap(k, j);
        }
        for (dx, dy) in d {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = ny as usize * w + nx as usize;
            if land[ni] != 1 || owner[ni] != -1 {
                continue;
            }
            owner[ni] = r;
            fr.push(ni);
        }
    }
    let mut region_kind: Vec<usize> = (0..region_count).map(|i| i % RESOURCE_KINDS).collect();
    for i in (1..region_kind.len()).rev() {
        let j = rng.range(i + 1);
        region_kind.swap(i, j);
    }
    let total: f32 = RARITY.iter().map(|r| r.1).sum();
    let region_rarity: Vec<usize> = (0..region_count)
        .map(|_| {
            let mut roll = rng.f() * total;
            for (t, r) in RARITY.iter().enumerate() {
                roll -= r.1;
                if roll <= 0.0 {
                    return t;
                }
            }
            0
        })
        .collect();
    for i in 0..n {
        if owner[i] >= 0 {
            let r = owner[i] as usize;
            out[i] = (region_kind[r] * 10 + region_rarity[r] + 11) as u8;
        }
    }
    out
}

impl Econ {
    pub fn new(map: &MapData, seed: u32) -> Econ {
        let (w, h) = (map.w, map.h);
        let n = map.countries.len();
        let mut rng = Rng(seed | 1);
        let resource_grid = compute_resource_map(&map.land, w, h, &mut rng);
        let mut stock = vec![[0.0f32; RES_N]; n];
        for c in 1..n {
            stock[c][Res::Money as usize] = STARTING_TREASURY_MIN.max(map.countries[c].gdp.max(1.0) * GDP_ANNUAL_YIELD / 12.0);
        }
        let (decision_mult, cap_div, cap_max) = if w <= 1000 { (1.0, 7.0, 18) } else { (1.65, 9.0, 16) };
        Econ {
            w,
            h,
            stock,
            buildings: Vec::new(),
            occ: vec![0; w * h],
            resource_grid,
            next_id: 1,
            ai_next: vec![0.0; n],
            rng,
            decision_mult,
            cap_div,
            cap_max,
            own_timer: 0.0,
        }
    }

    // ------------------------------------------------------------------ queries
    pub fn resource_at(&self, gx: i32, gy: i32) -> Option<(usize, usize)> {
        if gx < 0 || gy < 0 || gx as usize >= self.w || gy as usize >= self.h {
            return None;
        }
        let v = self.resource_grid[gy as usize * self.w + gx as usize] as i32;
        if v == 0 {
            return None;
        }
        Some((((v - 11) / 10) as usize, ((v - 11) % 10) as usize))
    }

    pub fn building_at(&self, cx: i32, cy: i32) -> Option<usize> {
        if cx < 0 || cy < 0 || cx as usize >= self.w || cy as usize >= self.h {
            return None;
        }
        let id = self.occ[cy as usize * self.w + cx as usize];
        if id == 0 {
            return None;
        }
        self.buildings.iter().position(|b| b.id == id)
    }

    fn level_mult(level: u8) -> f32 {
        (1.0 + OUTPUT_PER_LEVEL_MULT).powi(level as i32 - 1)
    }

    /// Per-year output of resource buildings (incl. deposit bonus). 0 for city / village.
    pub fn output_per_year(&self, b: &Building) -> f32 {
        let cat = &CATALOG[b.kind];
        let Some((_, per)) = cat.produces else { return 0.0 };
        let mut out = per * Self::level_mult(b.level);
        if let Some(k) = cat.spot {
            if let Some((rk, tier)) = self.resource_at(b.gx, b.gy) {
                if rk == k {
                    out *= RARITY[tier].2;
                }
            }
        }
        out
    }

    /// Millions of people per year added by one village.
    pub fn village_output(&self, b: &Building) -> f32 {
        VILLAGE_POP_PER_YEAR * Self::level_mult(b.level)
    }

    pub fn village_rate(&self, country: usize) -> f32 {
        self.buildings
            .iter()
            .filter(|b| b.country as usize == country && b.dismantle_at.is_none() && CATALOG[b.kind].special == Special::Village)
            .map(|b| self.village_output(b))
            .sum()
    }

    pub fn city_weight(&self, country: usize) -> f32 {
        self.buildings
            .iter()
            .filter(|b| b.country as usize == country && b.dismantle_at.is_none() && CATALOG[b.kind].special == Special::City)
            .map(|b| Self::level_mult(b.level))
            .sum()
    }

    /// _cityPopCapBonus: multiplier on the national population ceiling.
    pub fn city_cap_bonus(&self, country: usize) -> f32 {
        let weight = self.city_weight(country);
        if weight <= 0.0 {
            return 1.0;
        }
        let k = std::f32::consts::LN_2 / CITY_BONUS_HALF_LIFE_WEIGHT;
        1.0 + CITY_MAX_POP_BONUS * (1.0 - (-k * weight).exp())
    }

    pub fn build_cost(&self, kind: usize) -> Stock {
        let cat = &CATALOG[kind];
        let mut c = [0.0f32; RES_N];
        c[Res::Money as usize] = cat.money;
        for (r, v) in cat.need {
            c[*r as usize] = *v;
        }
        c
    }

    pub fn upgrade_cost(&self, b: &Building) -> Option<Stock> {
        let next = b.level + 1;
        if next > CATALOG[b.kind].max_level.min(MAX_LEVEL) || b.dismantle_at.is_some() {
            return None;
        }
        let mult = if next == 2 { 1.0 } else { 2.0 };
        let mut c = self.build_cost(b.kind);
        for v in c.iter_mut() {
            *v *= mult;
        }
        Some(c)
    }

    pub fn can_afford(&self, country: usize, cost: &Stock) -> bool {
        (0..RES_N).all(|r| cost[r] <= self.stock[country][r])
    }

    fn spend(&mut self, country: usize, cost: &Stock) {
        for r in 0..RES_N {
            self.stock[country][r] -= cost[r];
        }
    }

    /// coastSideCounts(...).best: how much open water touches the best side of a footprint.
    pub fn coast_best(&self, map: &MapData, gx: i32, gy: i32) -> i32 {
        let s = BUILDING_SIZE;
        let (w, h) = (self.w as i32, self.h as i32);
        let water = |x: i32, y: i32| -> i32 { if x >= 0 && y >= 0 && x < w && y < h && map.land[(y * w + x) as usize] == 0 { 1 } else { 0 } };
        let (mut south, mut west, mut north, mut east) = (0, 0, 0, 0);
        for d in 1..=PORT_WATER_REACH {
            for x in gx..gx + s {
                south += water(x, gy + s - 1 + d);
                north += water(x, gy - d);
            }
            for y in gy..gy + s {
                east += water(gx + s - 1 + d, y);
                west += water(gx - d, y);
            }
        }
        south.max(west).max(north).max(east)
    }

    /// Indices of `country`'s working ports (portsOf).
    pub fn ports_of(&self, map: &MapData, country: u16) -> Vec<usize> {
        (0..self.buildings.len())
            .filter(|&i| {
                let b = &self.buildings[i];
                b.kind == IDX_PORT && b.country == country && b.dismantle_at.is_none() && map.owner[self.port_anchor(b)] == country
            })
            .collect()
    }

    pub fn port_anchor(&self, b: &Building) -> usize {
        (b.gy + BUILDING_SIZE / 2) as usize * self.w + (b.gx + BUILDING_SIZE / 2) as usize
    }

    /// Open-water cells within reach of a port (where ships can leave from).
    pub fn port_water_cells(&self, map: &MapData, b: &Building) -> Vec<usize> {
        let mut out = Vec::new();
        for y in b.gy - PORT_WATER_REACH..b.gy + BUILDING_SIZE + PORT_WATER_REACH {
            for x in b.gx - PORT_WATER_REACH..b.gx + BUILDING_SIZE + PORT_WATER_REACH {
                if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
                    continue;
                }
                let i = y as usize * self.w + x as usize;
                if map.land[i] == 0 {
                    out.push(i);
                }
            }
        }
        out
    }

    pub fn can_place(&self, map: &MapData, gx: i32, gy: i32, country: u16, kind: Option<usize>) -> bool {
        if gx < 0 || gy < 0 || gx + BUILDING_SIZE > self.w as i32 || gy + BUILDING_SIZE > self.h as i32 {
            return false;
        }
        for y in gy..gy + BUILDING_SIZE {
            for x in gx..gx + BUILDING_SIZE {
                let i = y as usize * self.w + x as usize;
                if map.land[i] != 1 || map.owner[i] != country || self.occ[i] != 0 || map.crater[i] != 0 {
                    return false;
                }
            }
        }
        if let Some(k) = kind {
            if CATALOG[k].coastal && self.coast_best(map, gx, gy) <= 0 {
                return false;
            }
        }
        true
    }

    // ------------------------------------------------------------------ actions
    pub fn place(&mut self, map: &MapData, gx: i32, gy: i32, country: u16, kind: usize) -> Result<(), &'static str> {
        if !self.can_place(map, gx, gy, country, Some(kind)) {
            return Err("CAN'T BUILD THERE");
        }
        let cost = self.build_cost(kind);
        if !self.can_afford(country as usize, &cost) {
            return Err("NOT ENOUGH RESOURCES");
        }
        self.spend(country as usize, &cost);
        let id = self.next_id;
        self.next_id += 1;
        for y in gy..gy + BUILDING_SIZE {
            for x in gx..gx + BUILDING_SIZE {
                self.occ[y as usize * self.w + x as usize] = id;
            }
        }
        self.buildings.push(Building { id, kind, country, gx, gy, level: 1, dismantle_at: None });
        Ok(())
    }

    pub fn upgrade(&mut self, id: u32, country: u16) -> Result<u8, &'static str> {
        let Some(idx) = self.buildings.iter().position(|b| b.id == id) else { return Err("NO SUCH BUILDING") };
        if self.buildings[idx].country != country {
            return Err("NOT YOUR BUILDING");
        }
        let Some(cost) = self.upgrade_cost(&self.buildings[idx]) else { return Err("CAN'T UPGRADE") };
        if !self.can_afford(country as usize, &cost) {
            return Err("NOT ENOUGH RESOURCES");
        }
        self.spend(country as usize, &cost);
        self.buildings[idx].level += 1;
        Ok(self.buildings[idx].level)
    }

    /// Start taking a building down (it stops producing at once, disappears after dismantle_s).
    pub fn dismantle(&mut self, id: u32, country: u16, now: f32) -> Result<(), &'static str> {
        let Some(idx) = self.buildings.iter().position(|b| b.id == id) else { return Err("NO SUCH BUILDING") };
        if self.buildings[idx].country != country {
            return Err("NOT YOUR BUILDING");
        }
        if self.buildings[idx].dismantle_at.is_some() {
            return Err("ALREADY DISMANTLING");
        }
        self.buildings[idx].dismantle_at = Some(now + CATALOG[self.buildings[idx].kind].dismantle_s);
        Ok(())
    }

    // ------------------------------------------------------------------ tick
    pub fn step(&mut self, dt_days: f32, ctx: &EconCtx, map: &MapData, player: Option<u16>, time: f32) {
        self.finish_dismantles(time);
        self.regen(dt_days, ctx);
        self.own_timer += dt_days;
        if self.own_timer >= 2.0 {
            self.own_timer = 0.0;
            self.reassign(map);
        }
        self.ai_step(time, ctx, map, player);
    }

    fn finish_dismantles(&mut self, time: f32) {
        if !self.buildings.iter().any(|b| b.dismantle_at.map_or(false, |t| time >= t)) {
            return;
        }
        let done: Vec<u32> = self.buildings.iter().filter(|b| b.dismantle_at.map_or(false, |t| time >= t)).map(|b| b.id).collect();
        for v in self.occ.iter_mut() {
            if done.contains(v) {
                *v = 0;
            }
        }
        self.buildings.retain(|b| !done.contains(&b.id));
    }

    /// Buildings follow the land: whoever owns the centre cell owns the building.
    fn reassign(&mut self, map: &MapData) {
        for b in self.buildings.iter_mut() {
            let ci = (b.gy + BUILDING_SIZE / 2) as usize * self.w + (b.gx + BUILDING_SIZE / 2) as usize;
            let o = map.owner[ci];
            if o != 0 && o != b.country {
                b.country = o;
            }
        }
    }

    fn regen(&mut self, dt: f32, ctx: &EconCtx) {
        let n = self.stock.len();
        let mut fixed = vec![[0.0f32; RES_N]; n];
        let mut farm = vec![0.0f32; n];
        for b in &self.buildings {
            if b.dismantle_at.is_some() {
                continue;
            }
            let cat = &CATALOG[b.kind];
            let Some((res, _)) = cat.produces else { continue };
            let out = self.output_per_year(b);
            if cat.kind == "farm" {
                farm[b.country as usize] += out;
            } else {
                fixed[b.country as usize][res as usize] += out;
            }
        }
        for c in 1..n {
            if ctx.eliminated[c] {
                continue;
            }
            let pop = ctx.pop[c];
            let annual = (ctx.gdp[c] * GDP_ANNUAL_YIELD).max(MIN_ANNUAL_INCOME + pop * PER_POP_ANNUAL_INCOME);
            let orig = ctx.orig[c].max(1) as f32;
            let frac = (ctx.cells[c] as f32 / orig).clamp(0.0, 1.0);
            let territory = TERRITORY_FLOOR + (1.0 - TERRITORY_FLOOR) * frac;
            let capital = if ctx.fallen[c] { CAPITAL_FALLEN_MULT } else { 1.0 };
            let mut gains = [0.0f32; RES_N];
            gains[Res::Money as usize] = annual / 365.0 * territory * capital * ctx.mod_money[c];
            for r in 0..RES_N {
                gains[r] += fixed[c][r] / 365.0 * ctx.mod_prod[c];
            }
            gains[Res::Food as usize] += farm[c] / 365.0 * ctx.mod_prod[c] * farm_scale(pop);
            let demand = if pop > OUTPUT_REF_POP { OUTPUT_REF_POP * (pop / OUTPUT_REF_POP).powf(OUTPUT_SCALE_EXP) } else { pop.max(0.0) };
            gains[Res::Food as usize] -= demand * CIVILIAN_FOOD_PER_POP;
            let mat_cap = MATERIAL_BASE + MATERIAL_PER_REF_POP * demand;
            let food_cap = FOOD_BASE + FOOD_PER_REF_POP * demand;
            let money_cap = MONEY_MIN.max(annual);
            for r in 0..RES_N {
                let gain = gains[r] * dt;
                if gain == 0.0 {
                    continue;
                }
                let cap = if r == Res::Money as usize { money_cap } else if r == Res::Food as usize { food_cap } else { mat_cap };
                let cur = self.stock[c][r];
                let next = (cur + gain).max(0.0);
                self.stock[c][r] = if gain > 0.0 { next.min(cap.max(cur)) } else { next };
            }
        }
    }

    // ------------------------------------------------------------------ AI
    fn ai_step(&mut self, time: f32, ctx: &EconCtx, map: &MapData, player: Option<u16>) {
        let n = self.stock.len();
        for c in 1..n {
            if Some(c as u16) == player || ctx.eliminated[c] || ctx.fallen[c] || ctx.cells[c] == 0 {
                continue;
            }
            let jitter = 1.0 + (self.rng.f() - 0.5) * 2.0 * AI_JITTER;
            let next = self.ai_next[c];
            let after = time + (AI_INTERVAL_S * jitter * self.decision_mult).max(0.2);
            if next == 0.0 {
                self.ai_next[c] = after; // first sighting only schedules
                continue;
            }
            if time < next {
                continue;
            }
            self.ai_next[c] = after;
            self.ai_try_build(c, ctx, map);
        }
    }

    fn ai_try_build(&mut self, c: usize, ctx: &EconCtx, map: &MapData) {
        let (r1, r2, r3) = (self.rng.f(), self.rng.f(), self.rng.f());
        let want_port = self.ai_wants_port(c, ctx, map);
        let mine: Vec<usize> = (0..self.buildings.len())
            .filter(|&i| self.buildings[i].country as usize == c && self.buildings[i].dismantle_at.is_none())
            .collect();
        let count_of = |k: usize| mine.iter().filter(|&&i| self.buildings[i].kind == k).count();
        let afford = |k: usize| self.can_afford(c, &self.build_cost(k));
        let kind = if r1 < AI_SKILL {
            if count_of(IDX_FARM) == 0 {
                IDX_FARM
            } else if count_of(IDX_CONSTRUCTION) == 0 && afford(IDX_CONSTRUCTION) {
                IDX_CONSTRUCTION
            } else if count_of(IDX_METALFAB) == 0 && afford(IDX_METALFAB) {
                IDX_METALFAB
            } else if count_of(IDX_PORT) == 0 && want_port && afford(IDX_PORT) {
                IDX_PORT
            } else if count_of(IDX_CITY) <= count_of(IDX_BANK) {
                IDX_CITY
            } else {
                IDX_BANK
            }
        } else {
            let aff: Vec<usize> = (0..CATALOG.len()).filter(|&k| k != IDX_SILO && k != IDX_AIRDEF && afford(k)).collect();
            if aff.is_empty() {
                return;
            }
            aff[((r2 * aff.len() as f32) as usize).min(aff.len() - 1)]
        };
        let cost = self.build_cost(kind);
        let spendable = self.stock[c][Res::Money as usize] * (1.0 - AI_MONEY_RESERVE);
        let can_new = spendable >= cost[Res::Money as usize] && self.can_afford(c, &cost);
        let upgrade_id = mine
            .iter()
            .map(|&i| &self.buildings[i])
            .filter(|b| b.kind == kind)
            .find(|b| self.upgrade_cost(b).map_or(false, |uc| spendable >= uc[Res::Money as usize] && self.can_afford(c, &uc)))
            .map(|b| b.id);
        let active = mine.len();
        if let Some(id) = upgrade_id {
            if !can_new || r3 < AI_UPGRADE_BIAS {
                let _ = self.upgrade(id, c as u16);
                return;
            }
        }
        if !can_new {
            return;
        }
        let cap = ((2.0 + ((ctx.cells[c] as f32).sqrt() / self.cap_div).floor()) as usize).clamp(1, self.cap_max);
        if active >= cap {
            return;
        }
        if let Some((gx, gy)) = self.find_spot(c, kind, ctx, map) {
            let _ = self.place(map, gx, gy, c as u16, kind);
        }
    }

    fn find_spot(&mut self, c: usize, kind: usize, ctx: &EconCtx, map: &MapData) -> Option<(i32, i32)> {
        let bb = ctx.bbox[c];
        if bb[2] < bb[0] || bb[3] < bb[1] {
            return None;
        }
        let want = CATALOG[kind].spot;
        let mut fallback: Option<(i32, i32)> = None;
        let mut attempts = 0;
        let tries = if CATALOG[kind].coastal { 2500 } else { 200 };
        for _ in 0..tries {
            if attempts >= 6 {
                break;
            }
            let x = bb[0] + self.rng.range((bb[2] - bb[0] + 1) as usize) as i32;
            let y = bb[1] + self.rng.range((bb[3] - bb[1] + 1) as usize) as i32;
            if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
                continue;
            }
            if map.owner[y as usize * self.w + x as usize] as usize != c || !self.can_place(map, x, y, c as u16, Some(kind)) {
                continue;
            }
            attempts += 1;
            if fallback.is_none() {
                fallback = Some((x, y));
            }
            match want {
                Some(k) => {
                    if let Some((rk, _)) = self.resource_at(x, y) {
                        if rk == k {
                            return Some((x, y));
                        }
                    }
                }
                None => return Some((x, y)),
            }
        }
        fallback
    }

    /// Remove buildings (destroyed by a missile strike).
    pub fn remove_buildings(&mut self, ids: &[u32]) {
        for v in self.occ.iter_mut() {
            if *v != 0 && ids.contains(&*v) {
                *v = 0;
            }
        }
        self.buildings.retain(|b| !ids.contains(&b.id));
    }

    pub fn silos_of(&self, country: u16) -> Vec<usize> {
        (0..self.buildings.len())
            .filter(|&i| {
                let b = &self.buildings[i];
                b.kind == IDX_SILO && b.country == country && b.dismantle_at.is_none()
            })
            .collect()
    }

    /// AI helper: find a spot and build `kind` for country `c`.
    pub fn ai_place_kind(&mut self, c: usize, kind: usize, ctx: &EconCtx, map: &MapData) -> bool {
        match self.find_spot(c, kind, ctx, map) {
            Some((gx, gy)) => self.place(map, gx, gy, c as u16, kind).is_ok(),
            None => false,
        }
    }

    /// A random coastal spot for a port (self-test helper).
    pub fn any_coastal_spot(&mut self, map: &MapData, country: u16, ctx: &EconCtx) -> Option<(i32, i32)> {
        self.find_spot(country as usize, IDX_PORT, ctx, map)
    }

    /// _aiWantsPort: does this country have any coast at all?
    fn ai_wants_port(&mut self, c: usize, ctx: &EconCtx, map: &MapData) -> bool {
        let bb = ctx.bbox[c];
        if bb[2] < bb[0] || bb[3] < bb[1] {
            return false;
        }
        let (w, h) = (self.w as i32, self.h as i32);
        for _ in 0..400 {
            let x = bb[0] + self.rng.range((bb[2] - bb[0] + 1) as usize) as i32;
            let y = bb[1] + self.rng.range((bb[3] - bb[1] + 1) as usize) as i32;
            if x < 0 || y < 0 || x >= w || y >= h || map.owner[(y * w + x) as usize] as usize != c {
                continue;
            }
            let i = (y * w + x) as usize;
            let wu = self.w;
            if (x > 0 && map.land[i - 1] == 0) || (x < w - 1 && map.land[i + 1] == 0) || (y > 0 && map.land[i - wu] == 0) || (y < h - 1 && map.land[i + wu] == 0) {
                return true;
            }
        }
        false
    }

    /// A random spot where `country` can place a building (used by the self-test).
    pub fn any_spot(&mut self, map: &MapData, country: u16, ctx: &EconCtx) -> Option<(i32, i32)> {
        self.find_spot(country as usize, 0, ctx, map)
    }

    /// Resource-deposit overlay (RGBA) drawn while placing buildings.
    pub fn fill_overlay(&self, bytes: &mut [u8]) {
        let colors: [[u8; 3]; RESOURCE_KINDS] = [CATALOG[0].color, CATALOG[1].color, CATALOG[2].color, CATALOG[3].color, CATALOG[4].color];
        for i in 0..self.w * self.h {
            let v = self.resource_grid[i] as i32;
            if v == 0 {
                continue;
            }
            let kind = ((v - 11) / 10) as usize;
            let tier = ((v - 11) % 10) as usize;
            let c = colors[kind.min(RESOURCE_KINDS - 1)];
            bytes[i * 4] = c[0];
            bytes[i * 4 + 1] = c[1];
            bytes[i * 4 + 2] = c[2];
            bytes[i * 4 + 3] = (RARITY[tier.min(2)].3 * 255.0) as u8;
        }
    }
}
