//! Core war simulation: manpower, fronts, territory capture, capital fall, AI wars + peace.
//!
//! Ported from the JS `WarSim` (formulas and constants are the originals):
//!   frontPowerRatio / frontSpeedMult / effCostPerCell, _applyFrontAttrition,
//!   _updateDefenseAllocations / _defenseComponents, _regenManpower, _buildRegenCache,
//!   _runAiDeclarations, _aiWantsPeace, capital-fall collapse.
//! Supply lines (CONFIG.supply flood-fill readiness + attrition) are ported below.
//! Regimes + policy sliders (ideology.rs) feed combat speed, population growth, manpower regen, money, production.
//! NOT ported yet (approximated or skipped): dig-in/regroup realism extras, focused attacks,
//! naval invasions, alliances/assists, buildings/economy, missiles/nukes, ideology/policy,
//! empires, victory points, population sim, news feed, enclave capture.
use crate::econ::{Econ, EconCtx};
use crate::ideology;
use crate::mapdata::MapData;
use crate::world::World;
use std::collections::HashMap;

pub const SIM_STEP: f32 = 1.0 / 30.0;

// CONFIG.war
const MIN_COMMIT: f32 = 40000.0;
const COMMIT_FRACTION: f32 = 0.25;
const MP_COST_PER_CELL: f32 = 12.0;
const FRONT_CELLS_PER_SEC: f32 = 90.0;
const MIN_REMNANT_CELLS: usize = 3;
const POST_PEACE_WAR_LOCK_S: f32 = 20.0;
// CONFIG.combatBalance
const RATIO_CURVE: f32 = 0.72;
const ADVANCE_MIN_RATIO: f32 = 1.18;
const SPEED_MIN: f32 = 0.35;
const SPEED_MAX: f32 = 3.2;
const COST_MIN: f32 = 0.4;
const COST_MAX: f32 = 2.4;
const DEF_BASE: f32 = 0.009;
const ATT_BASE: f32 = 0.0065;
const MAX_DRAIN: f32 = 0.06;
const FIELD_DRAIN: f32 = 1.4;
const DEF_BUDGET: f32 = 0.85;
const ALLOC_SMOOTH_S: f32 = 3.0;
const RESERVE_W: f32 = 0.55;
const MIRROR_W: f32 = 1.0;
const OFFENSIVE_W: f32 = 0.4;
// CONFIG.capitalFall
const COLLAPSE_SPEED_MULT: f32 = 2.8;
const COLLAPSE_RAMP_S: f32 = 2.2;
const MIN_COLLAPSE_COST_FRAC: f32 = 0.08;
const COMMAND_ATTACK_WEAKNESS: f32 = 0.50;
const COMMAND_DEFENSE_WEAKNESS: f32 = 0.55;
const PEACE_LOSS_FRAC_BONUS: f32 = 0.35;
// CONFIG.manpowerRegen
const MAX_POOL_MULT: f32 = 2.5;
const FULL_RECOVER_MONTHS: f32 = 2.4;
const MIN_SEED_FRAC: f32 = 0.02;
const POP_FACTOR_REF: f32 = 20.0;
const POP_FACTOR_MIN: f32 = 0.85;
const POP_FACTOR_MAX: f32 = 2.2;
const TERRITORY_FLOOR: f32 = 0.15;
const CAPITAL_FALLEN_MULT: f32 = 0.5;
const SURGE_MAX: f32 = 2.6;
const SURGE_LOSS_REF: f32 = 0.5;
const SURGE_CAPITAL_BONUS: f32 = 0.8;
const MIN_OFFENSIVE_THROTTLE: f32 = 0.15;
// CONFIG.ai (medium difficulty)
const AI_MIN_AGGRESSION: f32 = 0.62;
const AI_DECLARE_INTERVAL_S: f32 = 45.0;
const AI_DECLARE_SCORE: f32 = 0.56;
const AI_PASS_S: f32 = 1.3;
const AI_ACCEPT_LOSS_W: f32 = 1.15;
const AI_MANPOWER_RATIO_W: f32 = 0.6;
const AI_ARMY_RESERVE: f32 = 0.15;
const AI_RETALIATE_BONUS: f32 = 1.6;
const AI_RETALIATE_CAP: f32 = 0.55;
const AI_COMMIT_COOLDOWN_S: f32 = 6.0;
/// Wars with no front on either side for this long are quietly ended (simplification).
const STALE_WAR_S: f32 = 60.0;
// CONFIG.troopFood
const UPKEEP_PER_MANPOWER_KT_DAY: f32 = 0.000011;
const TROOP_FOOD_RESERVE: f32 = 0.05;
const STARVE_RETURN_FRAC: f32 = 0.015;
const MIN_WAR_DAYS: f32 = 4.0;
// CONFIG.populationSim
const POP_GROWTH_FULL_MONTHS: f32 = 5.2;
const POP_PEACETIME_CAP_MULT: f32 = 1.8;
const POP_WAR_GROWTH_MULT: f32 = 0.55;
const POP_MIN_FRAC: f32 = 0.05;
const POP_LONG_TERM_GROWTH_PER_YEAR: f32 = 0.03;
const POP_LONG_TERM_MAX_MULT: f32 = 1.25;
const VILLAGE_MAX_POP_BONUS: f32 = 0.15;
const GROWTH_SCALE_MAX: f32 = 2.5;
const OUTPUT_SCALE_EXP: f32 = 0.7;
// alliances / war assists (CONFIG.ai, CONFIG.war)
const ALLIANCE_ACCEPT_THRESHOLD: f32 = 0.55;
const ALLIANCE_MP_WEIGHT: f32 = 0.9;
const ALLY_OFFER_S: f32 = 10.0;
const OFFER_COOLDOWN_S: f32 = 15.0;
const ASSIST_REQUEST_S: f32 = 15.0;
const ASSIST_AI_BASE: f32 = 0.80;
const ASSIST_AI_AGGRESSION_W: f32 = 0.20;
const ASSIST_AI_STRENGTH_W: f32 = 0.10;

// CONFIG.missile / CONFIG.airDefense / CONFIG.aiMissiles
const MISSILE_SPEED: f32 = 14.0;
const MISSILE_MIN_ALT: f32 = 20.0;
const MISSILE_MAX_ALT: f32 = 60.0;
const MISSILE_ALT_PER_DIST: f32 = 0.12;
const ROUTE_SAMPLES: usize = 96;
const MISSILE_PRELAUNCH_S: f32 = 5.0;
const MISSILE_COOLDOWN_S: f32 = 30.0;
const IMPACT_RADIUS: f32 = 17.0;
const IMPACT_VARIATION: f32 = 0.12;
const IMPACT_SPREAD_S: f32 = 1.8;
const URBAN_POP_SHARE: f32 = 0.9;
const CAPITAL_URBAN_WEIGHT: f32 = 1.5;
const URBAN_REACH_MULT: f32 = 1.15;
const URBAN_DAMAGE_CURVE: f32 = 1.35;
const URBAN_MIN_SEVERITY: f32 = 0.05;
const URBAN_LOCALITY_MULT: f32 = 6.0;
const URBAN_CITY_LOSS_CAP: f32 = 0.3;
const RURAL_LOSS_MULT: f32 = 2.6;
const RURAL_LOSS_MAX: f32 = 0.55;
const CAPITAL_SHOCK: f32 = 0.2;
const CAPITAL_CRISIS_MIN_SEVERITY: f32 = 0.15;
const MAX_POP_LOSS: f32 = 0.92;
const RESERVE_LOSS_MULT: f32 = 1.5;
const FRONT_MIN_LOSS_FRAC: f32 = 0.02;
const FRONT_MAX_LOSS_FRAC: f32 = 0.65;
const FRONT_BORDER_LOSS_MULT: f32 = 0.9;
const FRONT_DEFENDER_LOSS_MULT: f32 = 0.65;
const CRATER_HEAL_DAYS: f32 = 30.0;
const CRATER_HEAL_STEPS: u32 = 32;
const BURN_DAYS: f32 = 8.0;
const FADE_DAYS: f32 = 2.0;
const CAPITAL_CRISIS_DAYS: f32 = 90.0;
const CRISIS_DEFENSE_WEAKNESS: f32 = 0.6;
const CRISIS_FRONT_SPEED: f32 = 2.8;
const AD_CHECK_S: f32 = 0.24;
const AD_RANGE: [f32; 4] = [0.0, 24.0, 27.0, 30.0];
const AD_SPEED: f32 = 46.0;
const AD_RELOAD_S: [f32; 4] = [0.0, 4.6, 2.4, 1.3];
const AD_SHOTS: [u32; 4] = [0, 1, 2, 3];
const AD_BASE_HIT: [f32; 4] = [0.0, 0.43, 0.54, 0.64];
const AD_HIT_RADIUS: f32 = 4.5;
pub const MISSILE_COST_MONEY: f32 = 50.0;
pub const MISSILE_COST_OIL: f32 = 8.0;
pub const MISSILE_COST_URANIUM: f32 = 1.5;
const AIM_CHECK_S: f32 = 15.0;
const AIM_GLOBAL_GAP_S: f32 = 150.0;
const AIM_COUNTRY_GAP_S: f32 = 360.0;
const AIM_SAMPLES: usize = 12;
const AIM_BUILD: f32 = 0.0006;
const AIM_LAUNCH: f32 = 0.004;

// CONFIG.supply
const SUPPLY_RECHECK_TICKS: u64 = 45;
const SUPPLY_EXHAUST_DAYS: f32 = 24.0;
const SUPPLY_RECOVER_DAYS: f32 = 12.0;
const SUPPLY_MIN_READINESS: f32 = 0.22;
const SUPPLY_EXTRA_DAILY_ATTRITION: f32 = 0.012;
const SUPPLY_ATTRITION_CURVE: f32 = 1.45;
const SUPPLY_WARNING_LEVEL: f32 = 0.62;
const SUPPLY_RESTORED_LEVEL: f32 = 0.84;

// CONFIG.victoryPoints ("crowns") / CONFIG.annexation / CONFIG.ai
const VP_START_MIN: u32 = 6;
const VP_START_MAX: u32 = 16;
const VP_OCCUPY_CELLS_PER_POINT: f32 = 110.0;
const VP_BATTLE_WIN_MIN_CELLS: f32 = 45.0;
const VP_BATTLE_HELD_MAX_CELLS: f32 = 12.0;
const VP_BATTLE_WIN_POINTS: u32 = 2;
const VP_KILLS_PER_POINT: f32 = 160000.0;
const VP_MAX_POINTS: u32 = 1000;
const ANNEX_POP_WEIGHT: f32 = 3.9;
const ANNEX_POP_EXP: f32 = 0.6;
const ANNEX_GDP_WEIGHT: f32 = 0.8;
const ANNEX_GDP_EXP: f32 = 0.35;
const ANNEX_MIN_FULL_COST: u32 = 2;
const ANNEX_MAX_FULL_COST: u32 = 220;
const AI_BANK_PEACE_FRAC: f32 = 0.22;
const AI_FORCED_PEACE_LOSS: f32 = 0.8;
const FULL_OCCUPATION_FRAC: f32 = 0.90;
const AUTO_ANNEX_MIN_ATTACKER_FRAC: f32 = 0.20;

#[derive(Clone)]
pub struct MissileFlight {
    pub id: u32,
    pub country: u16,
    pub silo_id: u32,
    pub sx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
    pub tcell: usize,
    pub start_at: f32,
    pub impact_at: f32,
    pub intercepted_at: Option<f32>,
    pub ix: f32,
    pub iy: f32,
    pub impact_done: bool,
}

#[derive(Clone)]
pub struct Interceptor {
    pub battery: u32,
    pub country: u16,
    pub target: u32,
    pub sx: f32,
    pub sy: f32,
    pub ax: f32,
    pub ay: f32,
    pub start_at: f32,
    pub arrive_at: f32,
    pub chance: f32,
    /// Some(hit) once resolved
    pub resolved: Option<bool>,
    pub resolved_at: f32,
}

#[derive(Clone)]
pub struct Site {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub radius: i32,
    pub attacker: u16,
    pub impact_at: f32,
    pub apply_at: f32,
    pub applied: bool,
    pub burn_day: f32,
    pub cells: Vec<usize>,
    pub healed: bool,
    pub heal_stage: u32,
}

fn nuke_noise(x: i32, y: i32, seed: i32) -> f32 {
    let mut n = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)).wrapping_add(seed);
    n = (n ^ (((n as u32) >> 13) as i32)).wrapping_mul(1274126177);
    (((n ^ (((n as u32) >> 16) as i32)) as u32) as f32) / 4294967295.0
}

fn site_seed(id: u32) -> i32 {
    id.wrapping_mul(2654435761) as i32
}

fn missile_altitude(sx: f32, sy: f32, tx: f32, ty: f32) -> f32 {
    (((tx - sx).powi(2) + (ty - sy).powi(2)).sqrt() * MISSILE_ALT_PER_DIST).clamp(MISSILE_MIN_ALT, MISSILE_MAX_ALT)
}

fn route_len(sx: f32, sy: f32, tx: f32, ty: f32, h: f32) -> f32 {
    let mut prev = (sx, sy, 0.0f32);
    let mut len = 0.0f32;
    for i in 1..ROUTE_SAMPLES {
        let u = i as f32 / (ROUTE_SAMPLES - 1) as f32;
        let inv = 1.0 - u;
        let glide = u * u * (3.0 - 2.0 * u);
        let z = 3.0 * inv * inv * u * (h * 1.8) + 3.0 * inv * u * u * (h * 0.75);
        let p = (sx + (tx - sx) * glide, sy + (ty - sy) * glide, z);
        len += ((p.0 - prev.0).powi(2) + (p.1 - prev.1).powi(2) + (p.2 - prev.2).powi(2)).sqrt();
        prev = p;
    }
    len
}

/// Ground position of a missile at sim time `t`.
pub fn flight_pos(f: &MissileFlight, t: f32) -> (f32, f32) {
    let span = (f.impact_at - f.start_at).max(0.001);
    let u = ((t - f.start_at) / span).clamp(0.0, 1.0);
    let glide = u * u * (3.0 - 2.0 * u);
    (f.sx + (f.tx - f.sx) * glide, f.sy + (f.ty - f.sy) * glide)
}

pub fn air_defense_range(level: u8) -> f32 {
    AD_RANGE[(level as usize).clamp(1, 3)]
}

// CONFIG.naval
const NAVAL_LOAD_S: f32 = 5.2;
const NAVAL_MANPOWER_PER_BOAT: f32 = 4000.0;
const NAVAL_MAX_BOATS: u32 = 10;
const NAVAL_BOAT_CELLS_PER_SEC: f32 = 11.0;
const NAVAL_BEACHHEAD_CELLS: usize = 18;
const NAVAL_MAX_SEA_DIST: i32 = 900;
const NAVAL_SEARCH_BUDGET: usize = 400_000;
const NAVAL_REPLAN_S: f32 = 1.5;
const NAVAL_PATH_STRIDE: usize = 2;

/// A sea route + boats for an amphibious landing (front.naval in the JS).
#[derive(Clone, Default)]
pub struct Naval {
    pub embark: usize,
    pub land: Option<usize>,
    pub path: Vec<usize>,
    pub len: usize,
    pub load: f32,
    /// travel progress 0..1 of each boat at sea
    pub boats: Vec<f32>,
    pub pending: u32,
    pub manual: bool,
    pub ai: bool,
    pub port_id: Option<u32>,
}

#[derive(Clone)]
pub struct AllyOffer {
    pub from: u16,
    pub to: u16,
    pub created: f32,
}

#[derive(Clone)]
pub struct Assist {
    pub caller: u16,
    pub helper: u16,
    pub target: u16,
    pub created: f32,
}
// CONFIG.war (realism / mop-up)
const RECAPTURE_HOLD_TICKS: u64 = 3;
const MOPUP_MULT: f32 = 2.0;
const ENCLAVE_MAX_CELLS: usize = 40;
const ENCLAVE_MAX_FRAC: f32 = 0.1;
const ENC_RECHECK_TICKS: u64 = 8;
// CONFIG.combatBalance (dig-in / regroup)
const RATIO_PIVOT: f32 = 1.0;
const DIG_IN_SECONDS: f32 = 14.0;
const DIG_IN_MAX_BONUS: f32 = 0.55;
const STALL_CELLS_TRIGGER_FRAC: f32 = 0.035;
const STALL_MIN_CELLS: f32 = 60.0;
const REGROUP_ADVANCE_STRAIN: f32 = 0.35;
const REGROUP_RESISTANCE_STRAIN: f32 = 1.3;
const REGROUP_LOSS_STRAIN: f32 = 0.8;
const REGROUP_BASE_WORK: f32 = 0.15;
const REGROUP_RESISTANCE_WORK: f32 = 0.85;
const REGROUP_RECOVERY_PER_SEC: f32 = 0.15;
const REGROUP_RECOVERY_FLOOR: f32 = 0.4;
const REGROUP_DOMINANCE_RECOVERY_BONUS: f32 = 0.4;

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

#[derive(Default, Clone)]
pub struct Front {
    pub attacker: u16,
    pub defender: u16,
    pub committed: f32,
    pub def_alloc: f32,
    pub cell_budget: f32,
    pub collapse: bool,
    pub collapse_t: f32,
    pub frontier: Vec<usize>,
    pub rebuild_at: u64,
    pub done: bool,
    /// 0..1, grows while the front is not stalled; fortifies the defender
    pub dig_in: f32,
    pub regrouping: bool,
    pub regroup_work: f32,
    pub push_strain: f32,
    pub push_combat_losses: f32,
    /// supply readiness SUPPLY_MIN_READINESS..1 per side; follows the *_target values (flood-fill supply graph)
    pub att_supply: f32,
    pub def_supply: f32,
    pub att_supply_target: f32,
    pub def_supply_target: f32,
    att_supply_warned: bool,
    def_supply_warned: bool,
    /// cells this front has taken (crowns: battle outcome)
    vp_cells: f32,
    vp_settled: bool,
    enc_tick: u64,
    enc_pocket: Option<Vec<usize>>,
    pub naval: Option<Naval>,
    naval_replan_at: f32,
}

pub struct Sim {
    pub n: usize,
    pub names: Vec<String>,
    pub econ: Econ,
    /// live population in millions (population sim)
    pub pop: Vec<f32>,
    pop_base: Vec<f32>,
    prev_cells: Vec<u32>,
    gdp: Vec<f32>,
    live_gdp: Vec<f32>,
    density_ref: f32,
    allies: Vec<bool>,
    ally_cd: Vec<f32>,
    pub ally_offers: Vec<AllyOffer>,
    pub assists: Vec<Assist>,
    w: usize,
    h: usize,
    pub manpower: Vec<f32>,
    pub max_pool: Vec<f32>,
    regen_r: Vec<f32>,
    min_seed: Vec<f32>,
    pub cells: Vec<u32>,
    pub orig_cells: Vec<u32>,
    capital: Vec<Option<usize>>,
    pub capital_fallen: Vec<bool>,
    pub eliminated: Vec<bool>,
    at_war: Vec<bool>,
    peace_lock: Vec<f32>,
    adjacent: Vec<bool>,
    consider_cd: Vec<f32>,
    commit_cd: Vec<f32>,
    war_idle: Vec<f32>,
    offer_cd: Vec<f32>,
    pub aggression: Vec<f32>,
    pub fronts: Vec<Front>,
    pub player: Option<u16>,
    /// countries asking the player for peace
    pub peace_offers: Vec<u16>,
    pub game_days: f32,
    pub time: f32,
    pub speed: f32,
    pub paused: bool,
    tick: u64,
    acc: f32,
    rng: Rng,
    ai_cursor: usize,
    ai_timer: f32,
    adj_timer: f32,
    peace_timer: f32,
    bbox: Vec<[i32; 4]>,
    pub dirty: Vec<[i32; 4]>,
    pub log: Vec<(String, f32)>,
    recent: HashMap<usize, u64>,
    enc_seen: Vec<u32>,
    enc_epoch: u32,
    sea_dist: Vec<i32>,
    sea_queue: Vec<u32>,
    pub flights: Vec<MissileFlight>,
    pub interceptors: Vec<Interceptor>,
    pub sites: Vec<Site>,
    silo_cd: HashMap<u32, f32>,
    battery_cd: HashMap<u32, f32>,
    shots: HashMap<(u32, u32), u32>,
    next_flight_id: u32,
    ad_next: f32,
    ai_missile_next: f32,
    ai_missile_last_global: f32,
    ai_missile_last_country: Vec<f32>,
    crisis_until: Vec<f32>,
    crisis_strength: Vec<f32>,
    /// test hook: multiplies AI missile chances (CW_MISSILE_BOOST)
    missile_boost: f32,
    /// per-nation supplied-cell masks, rebuilt every SUPPLY_RECHECK_TICKS
    supply_reach: HashMap<u16, Vec<u8>>,
    supply_pool: Vec<Vec<u8>>,
    supply_queue: Vec<u32>,
    supply_last_check: u64,
    /// regime index into ideology::REGIMES, slider positions, last switch day, cached multiplier bundle per nation
    pub ideology: Vec<usize>,
    pub policy: Vec<[f32; 4]>,
    ideology_changed: Vec<f32>,
    mods: Vec<ideology::Mods>,
    mod_money: Vec<f32>,
    mod_prod: Vec<f32>,
    /// ===== empire system =====
    pub empire: Vec<bool>,
    pub empire_name: Vec<String>,
    pub empire_perks: Vec<Vec<u8>>,
    empire_ai_timer: f32,
    /// victory points ("crowns"): score + empire currency; progress buckets are [occupy cells, defensive kills]
    pub crowns: Vec<u32>,
    vp_progress: Vec<[f32; 2]>,
    /// wars ended by treaty whose occupied land is annexed on the next step (needs the World)
    pending_settle: Vec<(u16, u16)>,
    pub stat_annexed: u32,
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

impl Sim {
    pub fn new(world: &World, seed: u32) -> Sim {
        let m = &world.map;
        let n = m.countries.len();
        let (w, h) = (m.w, m.h);
        let mut cells = vec![0u32; n];
        let mut bbox = vec![[w as i32, h as i32, -1, -1]; n];
        for y in 0..h {
            for x in 0..w {
                let o = m.owner[y * w + x] as usize;
                if o == 0 {
                    continue;
                }
                cells[o] += 1;
                let b = &mut bbox[o];
                b[0] = b[0].min(x as i32);
                b[1] = b[1].min(y as i32);
                b[2] = b[2].max(x as i32);
                b[3] = b[3].max(y as i32);
            }
        }
        let mut rng = Rng(seed | 1);
        let mut manpower = vec![0.0f32; n];
        let mut max_pool = vec![1.0f32; n];
        let mut regen_r = vec![0.0f32; n];
        let mut min_seed = vec![1.0f32; n];
        let mut aggression = vec![0.5f32; n];
        let base_r = (19.0f32 * (1.0 - MIN_SEED_FRAC) / MIN_SEED_FRAC).ln() / (FULL_RECOVER_MONTHS * 30.0).max(1.0);
        for id in 1..n {
            let c = &m.countries[id];
            max_pool[id] = (c.mobilizable * MAX_POOL_MULT).max(1.0);
            manpower[id] = max_pool[id].min((c.military * 1000.0).round());
            let pop_factor = (c.pop.max(0.05) / POP_FACTOR_REF).sqrt().clamp(POP_FACTOR_MIN, POP_FACTOR_MAX);
            regen_r[id] = base_r * pop_factor;
            min_seed[id] = (max_pool[id] * MIN_SEED_FRAC).round().max(1.0);
            let base = rng.f();
            let jitter = (rng.f() - 0.5) * 0.22;
            aggression[id] = clamp01(base + jitter);
        }
        let mut capital: Vec<Option<usize>> = vec![None; n];
        for c in &m.capitals {
            capital[c.country as usize] = Some(c.y as usize * w + c.x as usize);
        }
        let names: Vec<String> = m.countries.iter().map(|c| c.name.to_uppercase()).collect();
        let empire_name_v: Vec<String> = names.iter().map(|nm| format!("{} EMPIRE", nm)).collect();
        let econ = Econ::new(m, seed ^ 0x9E37_79B9);
        let pop: Vec<f32> = m.countries.iter().map(|c| c.pop).collect();
        let gdp: Vec<f32> = m.countries.iter().map(|c| c.gdp).collect();
        let pop_base: Vec<f32> = pop.iter().map(|p| p.max(0.05)).collect();
        let live_gdp = gdp.clone();
        let prev_cells = cells.clone();
        let density_ref = pop_base.iter().skip(1).sum::<f32>() / (cells.iter().sum::<u32>() as f32).max(1.0);
        let orig_cells = cells.clone();
        let ideology_v: Vec<usize> = names.iter().map(|nm| ideology::roll_regime(nm, seed)).collect();
        let policy_v: Vec<[f32; 4]> = names.iter().map(|nm| ideology::roll_policy(nm, seed)).collect();
        let mods_v: Vec<ideology::Mods> = (0..n).map(|i| ideology::mods_for(ideology_v[i], &policy_v[i])).collect();
        let mod_money_v: Vec<f32> = mods_v.iter().map(|m| m[ideology::K_MONEY]).collect();
        let mod_prod_v: Vec<f32> = mods_v.iter().map(|m| m[ideology::K_PROD]).collect();
        let crowns_v: Vec<u32> = names
            .iter()
            .map(|nm| {
                let r = ideology::hash01(ideology::hash_str(&format!("{}::victoryPoints", nm)), seed);
                (VP_START_MIN + (r * (VP_START_MAX - VP_START_MIN + 1) as f32) as u32).min(VP_START_MAX)
            })
            .collect();
        let mut s = Sim {
            n,
            names,
            econ,
            pop,
            pop_base,
            prev_cells,
            gdp,
            live_gdp,
            density_ref,
            allies: vec![false; n * n],
            ally_cd: vec![0.0; n * n],
            ally_offers: Vec::new(),
            assists: Vec::new(),
            w,
            h,
            manpower,
            max_pool,
            regen_r,
            min_seed,
            cells,
            orig_cells,
            capital,
            capital_fallen: vec![false; n],
            eliminated: vec![false; n],
            at_war: vec![false; n * n],
            peace_lock: vec![0.0; n * n],
            adjacent: vec![false; n * n],
            consider_cd: vec![0.0; n * n],
            commit_cd: vec![0.0; n * n],
            war_idle: vec![0.0; n * n],
            offer_cd: vec![0.0; n],
            aggression,
            fronts: Vec::new(),
            player: None,
            peace_offers: Vec::new(),
            game_days: 0.0,
            time: 0.0,
            speed: 1.0,
            paused: true,
            tick: 0,
            acc: 0.0,
            rng,
            ai_cursor: 0,
            ai_timer: 0.0,
            adj_timer: 0.0,
            peace_timer: 0.0,
            bbox,
            dirty: Vec::new(),
            log: Vec::new(),
            recent: HashMap::new(),
            enc_seen: vec![0; w * h],
            enc_epoch: 0,
            sea_dist: Vec::new(),
            sea_queue: Vec::new(),
            flights: Vec::new(),
            interceptors: Vec::new(),
            sites: Vec::new(),
            silo_cd: HashMap::new(),
            battery_cd: HashMap::new(),
            shots: HashMap::new(),
            next_flight_id: 1,
            ad_next: 0.0,
            ai_missile_next: AIM_CHECK_S,
            ai_missile_last_global: -1000.0,
            ai_missile_last_country: vec![-1000.0; n],
            crisis_until: vec![0.0; n],
            crisis_strength: vec![0.0; n],
            missile_boost: std::env::var("CW_MISSILE_BOOST").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0),
            supply_reach: HashMap::new(),
            supply_pool: Vec::new(),
            supply_queue: Vec::new(),
            supply_last_check: u64::MAX,
            ideology: ideology_v,
            policy: policy_v,
            ideology_changed: vec![-ideology::COOLDOWN_DAYS; n],
            mods: mods_v,
            mod_money: mod_money_v,
            mod_prod: mod_prod_v,
            empire: vec![false; n],
            empire_name: empire_name_v,
            empire_perks: vec![Vec::new(); n],
            empire_ai_timer: 0.0,
            crowns: crowns_v,
            vp_progress: vec![[0.0; 2]; n],
            pending_settle: Vec::new(),
            stat_annexed: 0,
        };
        s.rebuild_adjacency(world);
        s
    }

    // ------------------------------------------------------------------ queries
    pub fn is_at_war(&self, a: u16, b: u16) -> bool {
        self.at_war[a as usize * self.n + b as usize]
    }
    pub fn are_adjacent(&self, a: u16, b: u16) -> bool {
        self.adjacent[a as usize * self.n + b as usize]
    }
    pub fn war_locked(&self, a: u16, b: u16) -> bool {
        self.peace_lock[a as usize * self.n + b as usize] > self.time
    }
    pub fn committed_total(&self, a: u16) -> f32 {
        self.fronts.iter().filter(|f| !f.done && f.attacker == a).map(|f| f.committed).sum()
    }
    fn is_human(&self, id: u16) -> bool {
        self.player == Some(id)
    }
    fn territory_scale(&self, id: usize) -> f32 {
        (self.cells[id] as f32 / self.orig_cells[id].max(1) as f32).clamp(0.35, 10.0)
    }
    pub fn effective_cap(&self, id: usize) -> f32 {
        self.max_pool[id] * self.territory_scale(id)
    }

    pub fn say(&mut self, s: String) {
        self.log.push((s, self.time));
        if self.log.len() > 12 {
            self.log.remove(0);
        }
    }

    // ------------------------------------------------------------------ combat math (statics from JS)
    fn power_ratio(&self, f: &Front) -> f32 {
        let (a, d) = (f.attacker as usize, f.defender as usize);
        let att_cmd = 1.0 - COMMAND_ATTACK_WEAKNESS * if self.capital_fallen[a] { 1.0 } else { 0.0 };
        let crisis = if self.game_days < self.crisis_until[d] { CRISIS_DEFENSE_WEAKNESS * self.crisis_strength[d] } else { 0.0 };
        let def_cmd = 1.0 - (COMMAND_DEFENSE_WEAKNESS * if self.capital_fallen[d] { 1.0 } else { 0.0 }).max(crisis);
        let att_sup = f.att_supply.clamp(SUPPLY_MIN_READINESS, 1.0);
        let def_sup = f.def_supply.clamp(SUPPLY_MIN_READINESS, 1.0);
        let att = (f.committed * att_cmd * att_sup).max(1.0);
        let dig = 1.0 + f.dig_in.clamp(0.0, 1.0) * DIG_IN_MAX_BONUS;
        let def = (f.def_alloc * def_cmd * dig * def_sup).max(1.0);
        (att / def).powf(RATIO_CURVE)
    }

    fn speed_mult(&self, f: &Front) -> f32 {
        // regime layer: the attacker's doctrine (attack) against the defender's (defense), applied once, here
        let i_mult = self.mods[f.attacker as usize][ideology::K_ATT] / self.mods[f.defender as usize][ideology::K_DEF].max(0.01);
        let mut ratio_mult = self.power_ratio(f).clamp(SPEED_MIN, SPEED_MAX) * i_mult;
        let dd = f.defender as usize;
        if self.game_days < self.crisis_until[dd] {
            ratio_mult *= 1.0 + (CRISIS_FRONT_SPEED - 1.0) * self.crisis_strength[dd];
        }
        if !f.collapse {
            return ratio_mult;
        }
        let t = clamp01(f.collapse_t / COLLAPSE_RAMP_S);
        let e = t * t * (3.0 - 2.0 * t);
        ratio_mult * (1.0 + (COLLAPSE_SPEED_MULT - 1.0) * e)
    }

    fn cost_per_cell(&self, f: &Front) -> f32 {
        let collapse_mult = self.speed_mult(f);
        let ratio_mult = self.power_ratio(f).clamp(COST_MIN, COST_MAX);
        let floor = MP_COST_PER_CELL * MIN_COLLAPSE_COST_FRAC;
        let base = floor.max(MP_COST_PER_CELL / collapse_mult);
        floor.max(base / ratio_mult)
    }

    fn defense_components(&self, defender: usize, vs: usize) -> (f32, f32) {
        let reserve = self.manpower[defender].max(0.0);
        let (mut mirror, mut elsewhere) = (0.0f32, 0.0f32);
        for f in &self.fronts {
            if f.done || f.attacker as usize != defender {
                continue;
            }
            if f.defender as usize == vs {
                mirror += f.committed.max(0.0);
            } else {
                elsewhere += f.committed.max(0.0);
            }
        }
        ((reserve * RESERVE_W + elsewhere * OFFENSIVE_W) * DEF_BUDGET, mirror * MIRROR_W)
    }

    fn update_defense_allocs(&mut self, dt: f32) {
        let t = clamp01(dt / ALLOC_SMOOTH_S);
        let mut updates: Vec<(usize, f32)> = Vec::new();
        for i in 0..self.fronts.len() {
            let f = &self.fronts[i];
            if f.done {
                continue;
            }
            let d = f.defender;
            let total: f32 = self.fronts.iter().filter(|g| !g.done && g.defender == d).map(|g| g.committed.max(1.0)).sum();
            let weight = f.committed.max(1.0) / total.max(1.0);
            let (shared, excl) = self.defense_components(d as usize, f.attacker as usize);
            let target = shared * weight + excl;
            updates.push((i, f.def_alloc + (target - f.def_alloc) * t));
        }
        for (i, v) in updates {
            self.fronts[i].def_alloc = v;
        }
    }

    // ------------------------------------------------------------------ actions
    pub fn declare_war(&mut self, a: u16, b: u16) -> bool {
        let (ai, bi) = (a as usize, b as usize);
        if a == b || self.eliminated[ai] || self.eliminated[bi] || self.at_war[ai * self.n + bi] || self.war_locked(a, b) {
            return false;
        }
        if self.are_allies(a, b) {
            return false;
        }
        if !self.has_war_food(a) {
            if self.is_human(a) {
                self.say("NOT ENOUGH FOOD TO GO TO WAR".to_string());
            }
            return false;
        }
        self.at_war[ai * self.n + bi] = true;
        self.at_war[bi * self.n + ai] = true;
        self.war_idle[ai * self.n + bi] = 0.0;
        self.war_idle[bi * self.n + ai] = 0.0;
        self.peace_offers.retain(|&x| x != a && x != b);
        let msg = format!("{} DECLARES WAR ON {}", self.names[ai], self.names[bi]);
        self.say(msg);
        self.offer_war_assist(a, b);
        if self.is_human(a) && !self.is_human(b) {
            self.ai_commit(b, a, true); // _aiRetaliate
        }
        true
    }

    /// Peace treaty: the war ends now, the occupied land each side holds is annexed on the next step.
    pub fn make_peace(&mut self, a: u16, b: u16) {
        self.end_war(a, b);
        self.pending_settle.push((a, b));
    }

    /// Ends the war state (flags, post-peace lock, fronts) without touching territory.
    fn end_war(&mut self, a: u16, b: u16) {
        let (ai, bi) = (a as usize, b as usize);
        self.at_war[ai * self.n + bi] = false;
        self.at_war[bi * self.n + ai] = false;
        self.peace_lock[ai * self.n + bi] = self.time + POST_PEACE_WAR_LOCK_S;
        self.peace_lock[bi * self.n + ai] = self.time + POST_PEACE_WAR_LOCK_S;
        for k in 0..self.fronts.len() {
            let (fa, fd) = (self.fronts[k].attacker, self.fronts[k].defender);
            if (fa == a && fd == b) || (fa == b && fd == a) {
                self.return_front(k);
            }
        }
        self.peace_offers.retain(|&x| x != a && x != b);
    }

    fn return_front(&mut self, k: usize) {
        let (att, amount) = (self.fronts[k].attacker as usize, self.fronts[k].committed.max(0.0));
        self.manpower[att] += amount;
        self.fronts[k].committed = 0.0;
        self.fronts[k].done = true;
    }

    /// Send `amount` troops from `att`'s reserve to the front against `def` (creates it if needed).
    pub fn commit(&mut self, att: u16, def: u16, amount: f32) -> f32 {
        let (a, d) = (att as usize, def as usize);
        let pool = self.manpower[a];
        let mut req = amount.min(pool).floor();
        let min_commit = pool.min(MIN_COMMIT);
        if req < min_commit {
            req = min_commit.floor();
        }
        if req <= 0.0 {
            return 0.0;
        }
        let idx = match self.fronts.iter().position(|f| f.attacker == att && f.defender == def && !f.done) {
            Some(i) => i,
            None => {
                let existing = self.fronts.iter().filter(|f| f.defender == def && !f.done).count();
                let (shared, excl) = self.defense_components(d, a);
                let f = Front {
                    attacker: att,
                    defender: def,
                    def_alloc: shared / (existing as f32 + 1.0) + excl,
                    collapse: self.capital_fallen[d],
                    att_supply: 1.0,
                    def_supply: 1.0,
                    att_supply_target: 1.0,
                    def_supply_target: 1.0,
                    ..Default::default()
                };
                self.fronts.push(f);
                self.fronts.len() - 1
            }
        };
        let room = (self.effective_cap(a) - self.fronts[idx].committed).max(0.0);
        let use_ = req.min(room);
        if use_ <= 0.0 {
            return 0.0;
        }
        self.manpower[a] -= use_;
        self.fronts[idx].committed += use_;
        use_
    }

    pub fn commit_fraction(&mut self, att: u16, def: u16, frac: f32) -> f32 {
        let amount = (self.manpower[att as usize] * frac).floor();
        self.commit(att, def, amount)
    }


    // ------------------------------------------------------------------ troop food + war food
    /// hasWarFood: you need enough food stock to feed the smallest legal commitment for a few days.
    pub fn has_war_food(&self, c: u16) -> bool {
        let pool = self.manpower[c as usize];
        if pool <= 0.0 {
            return true;
        }
        let food = self.econ.stock[c as usize][2];
        let spendable = (food - food * TROOP_FOOD_RESERVE).max(0.0);
        let minimum_commit = pool.min(MIN_COMMIT);
        spendable >= minimum_commit * UPKEEP_PER_MANPOWER_KT_DAY * MIN_WAR_DAYS
    }

    /// Committed troops eat food every day; unpaid upkeep sends them home.
    fn troop_food(&mut self, dt: f32) {
        let n = self.n;
        let mut committed = vec![0.0f32; n];
        for f in self.fronts.iter().filter(|f| !f.done) {
            committed[f.attacker as usize] += f.committed.max(0.0);
        }
        for c in 1..n {
            let total = committed[c];
            if total <= 0.0 {
                continue;
            }
            let due = total * UPKEEP_PER_MANPOWER_KT_DAY * dt;
            let food = self.econ.stock[c][2];
            let spendable = (food - food * TROOP_FOOD_RESERVE).max(0.0);
            let paid = due.min(spendable);
            if paid > 0.0 {
                self.econ.stock[c][2] = (food - paid).max(0.0);
            }
            let short = due - paid;
            if short <= 0.0 {
                continue;
            }
            let unpaid = clamp01(short / (total * UPKEEP_PER_MANPOWER_KT_DAY * dt).max(1e-12));
            let starving = total * (1.0 - (-STARVE_RETURN_FRAC * unpaid * dt).exp());
            if starving <= 0.0 {
                continue;
            }
            let mut returned = 0.0f32;
            for f in self.fronts.iter_mut().filter(|f| !f.done && f.attacker as usize == c) {
                let take = (starving * f.committed / total).min(f.committed);
                f.committed -= take;
                returned += take;
            }
            self.manpower[c] += returned;
            if self.is_human(c as u16) && returned > 1.0 {
                self.say("YOUR TROOPS ARE STARVING - BUILD FARMS".to_string());
            }
        }
    }

    // ------------------------------------------------------------------ population sim
    /// _updatePopulation: logistic growth towards a ceiling that follows territory, cities and villages.
    fn update_population(&mut self, dt: f32) {
        let n = self.n;
        let pop_r = (19.0f32).ln() / (POP_GROWTH_FULL_MONTHS * 30.0).max(1.0);
        let years = self.game_days.max(0.0) / 365.0;
        let long_term = (1.0 + POP_LONG_TERM_GROWTH_PER_YEAR).powf(years).min(POP_LONG_TERM_MAX_MULT);
        for id in 1..n {
            let cur = self.cells[id];
            let orig = self.orig_cells[id] as f32;
            if cur == 0 {
                self.pop[id] = 0.0;
                self.prev_cells[id] = 0;
                self.live_gdp[id] = 0.0;
                continue;
            }
            let base = self.pop_base[id];
            let mut pop = self.pop[id];
            let delta = cur as i64 - self.prev_cells[id] as i64;
            if delta != 0 && orig > 0.0 {
                let density = if delta > 0 { (base / orig).min(self.density_ref) } else { base / orig };
                pop = (pop + delta as f32 * density).max(0.0);
            }
            self.prev_cells[id] = cur;
            let territory = if orig > 0.0 { (cur as f32 / orig).clamp(0.0, 1.0) } else { 1.0 };
            let expansion_density = if orig > 0.0 { (base / orig).min(self.density_ref) } else { 0.0 };
            let acquired = (cur as f32 - orig).max(0.0) * expansion_density;
            let density_ceiling = base * POP_PEACETIME_CAP_MULT * territory.max(POP_MIN_FRAC) + acquired * POP_PEACETIME_CAP_MULT;
            let village_rate = self.econ.village_rate(id);
            let village_room = (village_rate * 10.0).min(density_ceiling * VILLAGE_MAX_POP_BONUS);
            let ceiling = (density_ceiling * long_term * self.econ.city_cap_bonus(id) + village_room).max(0.01);
            if pop > ceiling {
                pop = ceiling;
            }
            let at_war = self.fronts.iter().any(|f| !f.done && (f.attacker as usize == id || f.defender as usize == id));
            let r_eff = pop_r * if at_war { POP_WAR_GROWTH_MULT } else { 1.0 } * self.mods[id][ideology::K_POP];
            let rdt = r_eff * dt;
            let next = if rdt > 0.0 { pop * (1.0 + rdt) / (1.0 + rdt * pop / pop.max(ceiling)) } else { pop };
            let village_growth = village_rate * dt / 365.0;
            self.pop[id] = (next + village_growth).min(ceiling).max(0.01);
            // liveStatsOf: GDP follows land and population
            let land_scale = if orig > 0.0 { cur as f32 / orig } else { 1.0 };
            let pop_scale = if base > 0.0 { self.pop[id] / base } else { 1.0 };
            let raw = GROWTH_SCALE_MAX.min(land_scale.max(pop_scale).max(0.05));
            let scale = if raw > 1.0 { raw.powf(OUTPUT_SCALE_EXP) } else { raw };
            self.live_gdp[id] = self.gdp[id] * scale;
        }
    }

    // ------------------------------------------------------------------ alliances + war assists
    pub fn are_allies(&self, a: u16, b: u16) -> bool {
        self.allies[a as usize * self.n + b as usize]
    }

    pub fn ally_offer_pending(&self, a: u16, b: u16) -> bool {
        self.ally_offers.iter().any(|o| (o.from == a && o.to == b) || (o.from == b && o.to == a))
    }

    pub fn ally_on_cooldown(&self, a: u16, b: u16) -> bool {
        self.ally_cd[a as usize * self.n + b as usize] > self.time
    }

    /// requestAlly: the player proposes an alliance to another nation.
    pub fn request_ally(&mut self, player: u16, target: u16) -> bool {
        if player == target
            || self.eliminated[target as usize]
            || self.is_at_war(player, target)
            || self.are_allies(player, target)
            || self.ally_offer_pending(player, target)
            || self.ally_on_cooldown(player, target)
        {
            return false;
        }
        self.ally_offers.push(AllyOffer { from: player, to: target, created: self.time });
        let msg = format!("{} PROPOSES ALLIANCE TO {}", self.names[player as usize], self.names[target as usize]);
        self.say(msg);
        true
    }

    fn make_allies(&mut self, a: u16, b: u16) {
        let n = self.n;
        self.allies[a as usize * n + b as usize] = true;
        self.allies[b as usize * n + a as usize] = true;
        let msg = format!("{} ALLIED WITH {}", self.names[a as usize], self.names[b as usize]);
        self.say(msg);
    }

    /// _aiWantsAlliance
    fn ai_wants_alliance(&self, to: u16, from: u16) -> bool {
        let to_mp = self.manpower[to as usize];
        let from_mp = self.manpower[from as usize].max(1.0);
        let mp_ratio = clamp01(1.0 - to_mp / (to_mp + from_mp));
        let pressure = mp_ratio * ALLIANCE_MP_WEIGHT + (1.0 - self.aggression[to as usize]) * 0.4;
        pressure > ALLIANCE_ACCEPT_THRESHOLD
    }

    fn ally_pass(&mut self) {
        let n = self.n;
        let offers = std::mem::take(&mut self.ally_offers);
        let mut keep: Vec<AllyOffer> = Vec::new();
        for o in offers {
            let (to, from) = (o.to, o.from);
            if self.is_human(to) {
                keep.push(o);
                continue;
            }
            let reject = |s: &mut Sim| {
                s.ally_cd[to as usize * n + from as usize] = s.time + OFFER_COOLDOWN_S;
                s.ally_cd[from as usize * n + to as usize] = s.time + OFFER_COOLDOWN_S;
                let msg = format!("{} REJECTED ALLIANCE", s.names[to as usize]);
                s.say(msg);
            };
            if self.at_war[to as usize * n + from as usize] || self.eliminated[to as usize] {
                reject(self);
            } else if self.ai_wants_alliance(to, from) {
                self.make_allies(from, to);
            } else if self.time - o.created > ALLY_OFFER_S * 0.5 {
                reject(self);
            } else {
                keep.push(o);
            }
        }
        self.ally_offers = keep;
    }

    /// _offerWarAssist: when `caller` goes to war, its allies are asked to join.
    fn offer_war_assist(&mut self, caller: u16, target: u16) {
        let n = self.n;
        for h in 1..n {
            let hu = h as u16;
            if hu == caller || hu == target || !self.allies[caller as usize * n + h] {
                continue;
            }
            if self.at_war[h * n + target as usize] || self.allies[h * n + target as usize] || self.peace_lock[h * n + target as usize] > self.time || self.cells[h] == 0 || self.eliminated[h] {
                continue;
            }
            if self.assists.iter().any(|a| a.helper == hu && a.target == target) {
                continue;
            }
            self.assists.push(Assist { caller, helper: hu, target, created: self.time });
            if self.is_human(hu) {
                let msg = format!("{} ASKS YOU TO JOIN THE WAR ON {}", self.names[caller as usize], self.names[target as usize]);
                self.say(msg);
            }
        }
    }

    fn join_war_internal(&mut self, helper: u16, target: u16, caller: u16) {
        let n = self.n;
        let (h, t) = (helper as usize, target as usize);
        self.at_war[h * n + t] = true;
        self.at_war[t * n + h] = true;
        self.war_idle[h * n + t] = 0.0;
        self.war_idle[t * n + h] = 0.0;
        self.peace_offers.retain(|&x| x != helper && x != target);
        let msg = format!("{} JOINS {} AGAINST {}", self.names[h], self.names[caller as usize], self.names[t]);
        self.say(msg);
        if !self.is_human(target) {
            self.ai_commit(target, helper, true);
        }
    }

    pub fn join_war(&mut self, player: u16, target: u16) -> bool {
        let Some(pos) = self.assists.iter().position(|a| a.helper == player && a.target == target) else { return false };
        let req = self.assists.remove(pos);
        if !self.are_allies(player, req.caller) || !self.is_at_war(req.caller, target) || self.is_at_war(player, target) || self.are_allies(player, target) || self.war_locked(player, target) {
            return false;
        }
        self.join_war_internal(player, target, req.caller);
        true
    }

    pub fn decline_assist(&mut self, player: u16, target: u16) {
        if let Some(pos) = self.assists.iter().position(|a| a.helper == player && a.target == target) {
            self.assists.remove(pos);
            let msg = format!("{} STAYS OUT OF THE WAR ON {}", self.names[player as usize], self.names[target as usize]);
            self.say(msg);
        }
    }

    fn assist_pass(&mut self) {
        let reqs = std::mem::take(&mut self.assists);
        let mut keep: Vec<Assist> = Vec::new();
        for r in reqs {
            let (h, c, t) = (r.helper, r.caller, r.target);
            if !self.are_allies(h, c) || !self.is_at_war(c, t) || self.eliminated[h as usize] || self.eliminated[t as usize] {
                continue;
            }
            if self.is_human(h) {
                if self.time - r.created <= ASSIST_REQUEST_S {
                    keep.push(r);
                }
                continue;
            }
            let h_mp = self.manpower[h as usize];
            let t_mp = self.manpower[t as usize].max(1.0);
            let strength = clamp01(h_mp / (h_mp + t_mp).max(1.0));
            let chance = clamp01(ASSIST_AI_BASE + ASSIST_AI_AGGRESSION_W * self.aggression[h as usize] + ASSIST_AI_STRENGTH_W * (strength - 0.5));
            let roll = self.rng.f();
            if self.are_adjacent(h, t) && roll < chance && !self.is_at_war(h, t) && !self.war_locked(h, t) && !self.are_allies(h, t) {
                self.join_war_internal(h, t, c);
            } else {
                let msg = format!("{} STAYS OUT OF THE WAR ON {}", self.names[h as usize], self.names[t as usize]);
                self.say(msg);
            }
        }
        self.assists = keep;
    }



    // ------------------------------------------------------------------ missiles, silos, air defense
    pub fn silo_cooldown_left(&self, silo_id: u32) -> f32 {
        (self.silo_cd.get(&silo_id).copied().unwrap_or(0.0) - self.time).max(0.0)
    }

    pub fn can_afford_missile(&self, c: u16) -> bool {
        let st = &self.econ.stock[c as usize];
        st[0] >= MISSILE_COST_MONEY && st[4] >= MISSILE_COST_OIL && st[5] >= MISSILE_COST_URANIUM
    }

    pub fn war_enemies(&self, c: usize) -> Vec<u16> {
        (1..self.n).filter(|&e| self.at_war[c * self.n + e] && self.cells[e] > 0).map(|e| e as u16).collect()
    }

    /// startMissileFlight: fire a missile from a silo at a land cell.
    pub fn launch_missile(&mut self, player: u16, silo_id: u32, tx: i32, ty: i32, world: &World) -> Result<(), &'static str> {
        let Some(b) = self.econ.buildings.iter().find(|b| b.id == silo_id).cloned() else { return Err("NO SILO") };
        if b.kind != crate::econ::IDX_SILO || b.dismantle_at.is_some() || b.country != player {
            return Err("SILO NOT AVAILABLE");
        }
        let (w, h) = (self.w as i32, self.h as i32);
        if tx < 0 || ty < 0 || tx >= w || ty >= h || world.map.land[ty as usize * self.w + tx as usize] != 1 {
            return Err("PICK A TARGET ON LAND");
        }
        if self.silo_cooldown_left(silo_id) > 0.0 {
            return Err("SILO RELOADING");
        }
        if !self.can_afford_missile(player) {
            return Err("NOT ENOUGH RESOURCES TO LAUNCH");
        }
        let (sx, sy) = (b.gx as f32 + 2.0, b.gy as f32 + 2.0);
        let (txf, tyf) = (tx as f32 + 0.5, ty as f32 + 0.5);
        let alt = missile_altitude(sx, sy, txf, tyf);
        let duration = route_len(sx, sy, txf, tyf, alt) / MISSILE_SPEED;
        let st = &mut self.econ.stock[player as usize];
        st[0] -= MISSILE_COST_MONEY;
        st[4] -= MISSILE_COST_OIL;
        st[5] -= MISSILE_COST_URANIUM;
        let start_at = self.time + MISSILE_PRELAUNCH_S;
        let id = self.next_flight_id;
        self.next_flight_id += 1;
        self.flights.push(MissileFlight {
            id,
            country: player,
            silo_id,
            sx,
            sy,
            tx: txf,
            ty: tyf,
            tcell: ty as usize * self.w + tx as usize,
            start_at,
            impact_at: start_at + duration,
            intercepted_at: None,
            ix: 0.0,
            iy: 0.0,
            impact_done: false,
        });
        self.silo_cd.insert(silo_id, start_at + MISSILE_COOLDOWN_S);
        let msg = format!("{} LAUNCHES A MISSILE", self.names[player as usize]);
        self.say(msg);
        Ok(())
    }

    fn strike_war(&mut self, att: u16, target: u16) {
        let n = self.n;
        let (a, t) = (att as usize, target as usize);
        if a == t || self.eliminated[t] {
            return;
        }
        self.allies[a * n + t] = false;
        self.allies[t * n + a] = false;
        self.peace_offers.retain(|&x| x != att && x != target);
        if !self.at_war[a * n + t] {
            self.at_war[a * n + t] = true;
            self.at_war[t * n + a] = true;
            self.war_idle[a * n + t] = 0.0;
            self.war_idle[t * n + a] = 0.0;
            let msg = format!("{} STRIKES {}", self.names[a], self.names[t]);
            self.say(msg);
            if !self.is_human(target) {
                self.ai_commit(target, att, true);
            }
        }
    }

    fn tick_missiles(&mut self, world: &mut World) {
        let now = self.time;
        if !self.flights.is_empty() && now >= self.ad_next {
            self.ad_next = now + AD_CHECK_S;
            self.air_defense_scan(world);
        }
        for k in 0..self.interceptors.len() {
            if self.interceptors[k].resolved.is_some() || now < self.interceptors[k].arrive_at {
                continue;
            }
            let it = self.interceptors[k].clone();
            let mut hit = false;
            if let Some(fi) = self.flights.iter().position(|f| f.id == it.target) {
                if self.flights[fi].intercepted_at.is_none() && now < self.flights[fi].impact_at {
                    let (px, py) = flight_pos(&self.flights[fi], now);
                    let dist = ((px - it.ax).powi(2) + (py - it.ay).powi(2)).sqrt();
                    let roll = self.rng.f();
                    if dist <= AD_HIT_RADIUS && roll < it.chance.clamp(0.05, 0.86) {
                        hit = true;
                        self.flights[fi].intercepted_at = Some(now);
                        self.flights[fi].ix = px;
                        self.flights[fi].iy = py;
                        let msg = format!("{} MISSILE INTERCEPTED", self.names[self.flights[fi].country as usize]);
                        self.say(msg);
                    }
                }
            }
            self.interceptors[k].resolved = Some(hit);
            self.interceptors[k].resolved_at = now;
        }
        self.interceptors.retain(|i| i.resolved.map_or(true, |_| now - i.resolved_at < 2.0));
        for fi in 0..self.flights.len() {
            let f = self.flights[fi].clone();
            if f.intercepted_at.is_some() || f.impact_done || now < f.impact_at {
                continue;
            }
            self.flights[fi].impact_done = true;
            let seed = site_seed(f.id);
            let variation = nuke_noise(f.tx as i32, f.ty as i32, seed);
            let radius = ((IMPACT_RADIUS * (1.0 + (variation * 2.0 - 1.0) * IMPACT_VARIATION)).round() as i32).max(1);
            self.sites.push(Site { id: f.id, x: f.tx, y: f.ty, radius, attacker: f.country, impact_at: f.impact_at, apply_at: f.impact_at + IMPACT_SPREAD_S, applied: false, burn_day: 0.0, cells: Vec::new(), healed: false, heal_stage: 0 });
            let owner = world.map.owner[f.tcell];
            if owner != 0 && owner != f.country {
                self.strike_war(f.country, owner);
            }
        }
        for si in 0..self.sites.len() {
            if !self.sites[si].applied && now >= self.sites[si].apply_at {
                self.apply_impact(si, world);
            }
        }
        for si in 0..self.sites.len() {
            let s = &self.sites[si];
            if !s.applied || s.healed {
                continue;
            }
            let start = BURN_DAYS + FADE_DAYS;
            let age = self.game_days - s.burn_day;
            let stage = (CRATER_HEAL_STEPS as f32 * ((age - start) / (CRATER_HEAL_DAYS - start).max(1.0)).clamp(0.0, 1.0)).floor() as u32;
            if stage > s.heal_stage {
                let level = (255.0 * (1.0 - stage as f32 / CRATER_HEAL_STEPS as f32)) as u8;
                let cells = self.sites[si].cells.clone();
                let mut bx = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
                for &c in &cells {
                    if world.map.crater[c] > level {
                        world.map.crater[c] = level;
                    }
                    let (x, y) = ((c % self.w) as i32, (c / self.w) as i32);
                    bx = [bx[0].min(x), bx[1].min(y), bx[2].max(x), bx[3].max(y)];
                }
                self.sites[si].heal_stage = stage;
                self.sites[si].healed = stage >= CRATER_HEAL_STEPS;
                if bx[0] <= bx[2] {
                    self.dirty.push(bx);
                }
            }
        }
        self.flights.retain(|f| match f.intercepted_at {
            Some(t) => now < t + 2.1,
            None => !f.impact_done || now < f.impact_at + 5.0,
        });
        if self.sites.len() > 64 {
            let day = self.game_days;
            self.sites.retain(|s| !s.healed || day - s.burn_day < 180.0);
        }
        let live: Vec<u32> = self.flights.iter().map(|f| f.id).collect();
        self.shots.retain(|(fid, _), _| live.contains(fid));
    }

    fn air_defense_scan(&mut self, world: &World) {
        let now = self.time;
        let batteries: Vec<(u32, u16, f32, f32, usize)> = self
            .econ
            .buildings
            .iter()
            .filter(|b| b.kind == crate::econ::IDX_AIRDEF && b.dismantle_at.is_none())
            .map(|b| (b.id, b.country, b.gx as f32 + 2.0, b.gy as f32 + 2.0, (b.level as usize).clamp(1, 3)))
            .collect();
        for (bid, bc, bx, by, lvl) in batteries {
            if self.interceptors.len() >= 64 {
                break;
            }
            if self.battery_cd.get(&bid).copied().unwrap_or(0.0) > now {
                continue;
            }
            let range = AD_RANGE[lvl];
            let mut chosen: Option<usize> = None;
            let mut closest = f32::MAX;
            for (fi, f) in self.flights.iter().enumerate() {
                if f.intercepted_at.is_some() || now < f.start_at || now >= f.impact_at - 0.4 {
                    continue;
                }
                if f.country == bc || self.are_allies(f.country, bc) {
                    continue;
                }
                let owner = world.map.owner[f.tcell];
                if owner != bc && !(owner != 0 && self.are_allies(owner, bc)) {
                    continue;
                }
                if self.shots.get(&(f.id, bid)).copied().unwrap_or(0) >= AD_SHOTS[lvl] {
                    continue;
                }
                let (px, py) = flight_pos(f, now);
                let hz = ((px - bx).powi(2) + (py - by).powi(2)).sqrt();
                if hz > range {
                    continue;
                }
                let urgency = (f.impact_at - now) + hz * 0.02;
                if urgency < closest {
                    closest = urgency;
                    chosen = Some(fi);
                }
            }
            let Some(fi) = chosen else { continue };
            let f = self.flights[fi].clone();
            let (mut ax, mut ay) = flight_pos(&f, now);
            let mut dur = 0.0f32;
            for _ in 0..3 {
                dur = (((ax - bx).powi(2) + (ay - by).powi(2)).sqrt() / AD_SPEED).max(0.35);
                let p = flight_pos(&f, now + dur);
                ax = p.0;
                ay = p.1;
            }
            if now + dur >= f.impact_at - 0.15 {
                continue;
            }
            *self.shots.entry((f.id, bid)).or_insert(0) += 1;
            self.battery_cd.insert(bid, now + AD_RELOAD_S[lvl]);
            let chance = AD_BASE_HIT[lvl] - 0.08 * (((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()) / range;
            self.interceptors.push(Interceptor { battery: bid, country: bc, target: f.id, sx: bx, sy: by, ax, ay, start_at: now, arrive_at: now + dur, chance, resolved: None, resolved_at: 0.0 });
        }
    }

    fn site_contains(&self, s: &Site, x: i32, y: i32) -> bool {
        let r = s.radius as f32;
        let (dx, dy) = (x as f32 + 0.5 - s.x, y as f32 + 0.5 - s.y);
        if dx.abs() > r || dy.abs() > r {
            return false;
        }
        (dx * dx + dy * dy).sqrt() <= r * (0.91 + 0.18 * nuke_noise(x, y, site_seed(s.id)))
    }

    /// _applyMissileImpact: craters, population + reserve losses, front casualties, destroyed buildings,
    /// capital crisis. Ownership never changes: only war fronts take land.
    fn apply_impact(&mut self, si: usize, world: &mut World) {
        let (sx, sy, r, att, sid) = {
            let s = &self.sites[si];
            (s.x, s.y, s.radius, s.attacker, s.id)
        };
        let (w, h) = (self.w as i32, self.h as i32);
        let wu = self.w;
        let seed = site_seed(sid);
        let rf = r as f32;
        let mut hit: Vec<usize> = Vec::new();
        let mut exposure: HashMap<u16, f32> = HashMap::new();
        let mut bx = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        let y0 = ((sy - rf).floor() as i32).max(0);
        let y1 = ((sy + rf).ceil() as i32).min(h - 1);
        let x0 = ((sx - rf).floor() as i32).max(0);
        let x1 = ((sx + rf).ceil() as i32).min(w - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = y as usize * wu + x as usize;
                if world.map.land[i] != 1 {
                    continue;
                }
                let dist = ((x as f32 + 0.5 - sx).powi(2) + (y as f32 + 0.5 - sy).powi(2)).sqrt();
                let edge = 0.91 + 0.18 * nuke_noise(x, y, seed);
                if dist <= rf * edge {
                    hit.push(i);
                    let o = world.map.owner[i];
                    if o != 0 {
                        *exposure.entry(o).or_insert(0.0) += (1.0 - dist / rf).max(0.0).powf(1.4);
                    }
                    bx = [bx[0].min(x), bx[1].min(y), bx[2].max(x), bx[3].max(y)];
                }
            }
        }
        self.sites[si].applied = true;
        self.sites[si].burn_day = self.game_days;
        if hit.is_empty() {
            return;
        }
        struct Urban {
            all: f32,
            hit: f32,
            cap_sev: f32,
        }
        let mut urban: HashMap<u16, Urban> = HashMap::new();
        for cap in &world.map.capitals {
            let owner = world.map.owner[cap.y as usize * wu + cap.x as usize];
            if owner == 0 {
                continue;
            }
            let (mx, my) = (cap.x as f32 + 1.0, cap.y as f32 + 1.0);
            let reach = rf * URBAN_REACH_MULT;
            let prox = (1.0 - ((mx - sx).powi(2) + (my - sy).powi(2)).sqrt() / reach.max(1.0)).clamp(0.0, 1.0);
            let sev = prox.powf(URBAN_DAMAGE_CURVE);
            let it = urban.entry(owner).or_insert(Urban { all: 0.0, hit: 0.0, cap_sev: 0.0 });
            it.all += CAPITAL_URBAN_WEIGHT;
            if sev >= URBAN_MIN_SEVERITY {
                it.hit += CAPITAL_URBAN_WEIGHT * sev;
                it.cap_sev = it.cap_sev.max(sev);
            }
        }
        let mut affected: Vec<u16> = exposure.keys().copied().collect();
        for (c, u) in &urban {
            if u.hit > 0.0 && !affected.contains(c) {
                affected.push(*c);
            }
        }
        let mut hitmask = vec![false; wu * self.h];
        for &i in &hit {
            hitmask[i] = true;
        }
        self.impact_front_casualties(&hitmask, world);
        for &i in &hit {
            world.map.crater[i] = 255;
        }
        for &c in &affected {
            if c != att {
                self.strike_war(att, c);
            }
        }
        let mut total_dead = 0.0f32;
        let mut target_name = String::new();
        for &c in &affected {
            let ci = c as usize;
            let before = (self.cells[ci].max(1)) as f32;
            let direct = RURAL_LOSS_MAX.min(RURAL_LOSS_MULT * exposure.get(&c).copied().unwrap_or(0.0) / before);
            let (mut urban_loss, mut capital_shock, mut cap_sev) = (0.0f32, 0.0f32, 0.0f32);
            if let Some(u) = urban.get(&c) {
                let footprint = std::f32::consts::PI * (rf * URBAN_REACH_MULT).powi(2);
                let locality = (URBAN_LOCALITY_MULT * footprint / before).clamp(0.0, 1.0);
                if u.all > 0.0 {
                    urban_loss = URBAN_CITY_LOSS_CAP.min(URBAN_POP_SHARE * u.hit / u.all * locality);
                }
                capital_shock = CAPITAL_SHOCK * u.cap_sev;
                cap_sev = u.cap_sev;
            }
            if cap_sev >= CAPITAL_CRISIS_MIN_SEVERITY && !self.capital_fallen[ci] {
                self.crisis_strength[ci] = self.crisis_strength[ci].max(cap_sev);
                self.crisis_until[ci] = self.crisis_until[ci].max(self.game_days + CAPITAL_CRISIS_DAYS);
                let msg = format!("{}'S CAPITAL STRUCK - DEFENSES IN CRISIS", self.names[ci]);
                self.say(msg);
            }
            let loss = MAX_POP_LOSS.min(1.0 - (1.0 - direct) * (1.0 - urban_loss) * (1.0 - capital_shock));
            let pop = self.pop[ci];
            let after = (pop * (1.0 - loss)).max(0.01);
            total_dead += (pop - after).max(0.0) * 1.0e6;
            self.pop[ci] = after;
            self.manpower[ci] = (self.manpower[ci] * (1.0 - (loss * RESERVE_LOSS_MULT).min(0.98))).max(0.0);
            if target_name.is_empty() {
                target_name = self.names[ci].clone();
            }
        }
        let site = self.sites[si].clone();
        let doomed: Vec<u32> = self
            .econ
            .buildings
            .iter()
            .filter(|b| self.site_contains(&site, b.gx + 2, b.gy + 2))
            .map(|b| b.id)
            .collect();
        if !doomed.is_empty() {
            self.econ.remove_buildings(&doomed);
        }
        self.sites[si].cells = hit;
        self.dirty.push(bx);
        let casualties = if total_dead >= 1.0e6 { format!("{:.1}M", total_dead / 1.0e6) } else { format!("{:.0}K", total_dead / 1.0e3) };
        let msg = format!("{} STRIKES {}: {} DEAD, {} BUILDINGS DESTROYED", self.names[att as usize], target_name, casualties, doomed.len());
        self.say(msg);
    }

    /// _impactFrontCasualties: troops fighting along the blasted border are hit.
    fn impact_front_casualties(&mut self, hitmask: &[bool], world: &World) {
        let (w, h) = (self.w as i32, self.h as i32);
        let wu = self.w;
        let owner = &world.map.owner;
        let adjacent = |i: usize, other: u16| -> bool {
            let (x, y) = ((i % wu) as i32, (i / wu) as i32);
            (x > 0 && owner[i - 1] == other) || (x < w - 1 && owner[i + 1] == other) || (y > 0 && owner[i - wu] == other) || (y < h - 1 && owner[i + wu] == other)
        };
        for fi in 0..self.fronts.len() {
            let (att, def, committed, def_alloc, done) = {
                let f = &self.fronts[fi];
                (f.attacker, f.defender, f.committed, f.def_alloc, f.done)
            };
            if done || committed <= 0.0 {
                continue;
            }
            let (mut hit_att, mut hit_def, mut tot_att, mut tot_def) = (0u32, 0u32, 0u32, 0u32);
            let ba = self.bbox[att as usize];
            let bd = self.bbox[def as usize];
            let (x0, y0, x1, y1) = (ba[0].min(bd[0]).max(0), ba[1].min(bd[1]).max(0), ba[2].max(bd[2]).min(w - 1), ba[3].max(bd[3]).min(h - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let i = y as usize * wu + x as usize;
                    let o = owner[i];
                    if o == att && adjacent(i, def) {
                        tot_att += 1;
                        if hitmask[i] {
                            hit_att += 1;
                        }
                    } else if o == def && adjacent(i, att) {
                        tot_def += 1;
                        if hitmask[i] {
                            hit_def += 1;
                        }
                    }
                }
            }
            if hit_att == 0 && hit_def == 0 {
                continue;
            }
            let exposed = (hit_att as f32 / tot_att.max(1) as f32).max(hit_def as f32 / tot_def.max(1) as f32);
            let fraction = (exposed * FRONT_BORDER_LOSS_MULT).clamp(FRONT_MIN_LOSS_FRAC, FRONT_MAX_LOSS_FRAC);
            let att_loss = committed * fraction;
            self.fronts[fi].committed = (committed - att_loss).max(0.0);
            let pool = self.manpower[def as usize];
            let def_loss = pool.min(def_alloc * fraction * FRONT_DEFENDER_LOSS_MULT);
            self.manpower[def as usize] = (pool - def_loss).max(0.0);
        }
    }

    // ------------------------------------------------------------------ AI missiles
    fn ai_missile_target(&mut self, c: usize, enemies: &[u16], world: &World) -> Option<(i32, i32)> {
        let (w, h) = (self.w as i32, self.h as i32);
        let wu = self.w;
        let radius = (IMPACT_RADIUS * (1.0 + IMPACT_VARIATION)).ceil() as i32 + 1;
        let r2 = radius * radius;
        let mut cands: Vec<(i32, i32, u16, f32)> = Vec::new();
        for cap in &world.map.capitals {
            if enemies.contains(&cap.country) {
                cands.push((cap.x as i32 + 1, cap.y as i32 + 1, cap.country, 6.0));
            }
        }
        for &e in enemies {
            let bb = self.bbox[e as usize];
            if bb[2] < bb[0] {
                continue;
            }
            let mut got = 0;
            for _ in 0..AIM_SAMPLES * 6 {
                if got >= AIM_SAMPLES {
                    break;
                }
                let x = bb[0] + self.rng.range((bb[2] - bb[0] + 1) as usize) as i32;
                let y = bb[1] + self.rng.range((bb[3] - bb[1] + 1) as usize) as i32;
                if x >= 0 && y >= 0 && x < w && y < h && world.map.owner[y as usize * wu + x as usize] == e {
                    cands.push((x, y, e, 1.0));
                    got += 1;
                }
            }
        }
        let mut best: Option<(i32, i32)> = None;
        let mut best_score = f32::MIN;
        for (gx, gy, e, base) in cands {
            if gx < 0 || gy < 0 || gx >= w || gy >= h {
                continue;
            }
            let center = gy as usize * wu + gx as usize;
            if world.map.owner[center] != e || world.map.land[center] != 1 || world.map.crater[center] != 0 {
                continue;
            }
            let mut safe = true;
            let mut area = 0i32;
            'scan: for dy in -radius..=radius {
                let y = gy + dy;
                if y < 0 || y >= h {
                    continue;
                }
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > r2 {
                        continue;
                    }
                    let x = gx + dx;
                    if x < 0 || x >= w {
                        continue;
                    }
                    let o = world.map.owner[y as usize * wu + x as usize];
                    if o == 0 {
                        continue;
                    }
                    if !enemies.contains(&o) {
                        safe = false;
                        break 'scan;
                    }
                    area += 1;
                }
            }
            if !safe || (area as f32) < (radius * radius) as f32 * 0.35 {
                continue;
            }
            if self.sites.iter().any(|s| s.applied && !s.healed && ((s.x - gx as f32).powi(2) + (s.y - gy as f32).powi(2)).sqrt() < s.radius as f32 + radius as f32 * 0.5) {
                continue;
            }
            let pressure = if self.fronts.iter().any(|f| f.attacker == e && f.defender as usize == c && f.committed > 0.0) { 2.0 } else { 0.0 };
            let score = base + pressure + (area as f32 / (std::f32::consts::PI * r2 as f32)).min(1.0) + self.rng.f() * 0.2;
            if score > best_score {
                best_score = score;
                best = Some((gx, gy));
            }
        }
        best
    }

    fn econ_ai_place(&mut self, c: usize, kind: usize, world: &World) -> bool {
        let ctx = EconCtx {
            names: &self.names,
            cells: &self.cells,
            orig: &self.orig_cells,
            fallen: &self.capital_fallen,
            eliminated: &self.eliminated,
            pop: &self.pop,
            gdp: &self.live_gdp,
            bbox: &self.bbox,
                mod_money: &self.mod_money,
                mod_prod: &self.mod_prod,
        };
        self.econ.ai_place_kind(c, kind, &ctx, &world.map)
    }

    /// _runAiMissiles + _aiTryBuildSilo + _aiTryBuildAirDefense (medium difficulty).
    fn ai_missiles(&mut self, world: &World) {
        use crate::econ::{IDX_AIRDEF, IDX_SILO};
        let now = self.time;
        if now < self.ai_missile_next {
            return;
        }
        self.ai_missile_next = now + AIM_CHECK_S;
        let n = self.n;
        let rate = if self.w <= 1000 { 1.0f32 } else { 0.65 };
        let max_silos = if self.w <= 1000 { 2 } else { 4 };
        let boost = self.missile_boost;
        let mut silos_now = self.econ.buildings.iter().filter(|b| b.kind == IDX_SILO && b.dismantle_at.is_none() && Some(b.country) != self.player).count();
        for c in 1..n {
            if self.eliminated[c] || self.is_human(c as u16) || self.capital_fallen[c] || self.cells[c] == 0 {
                continue;
            }
            let enemies = self.war_enemies(c);
            if enemies.is_empty() {
                continue;
            }
            let has_silo = !self.econ.silos_of(c as u16).is_empty();
            if !has_silo && silos_now < max_silos && self.rng.f() < AIM_BUILD * rate * boost {
                let cost = self.econ.build_cost(IDX_SILO);
                if self.econ.can_afford(c, &cost) && self.econ_ai_place(c, IDX_SILO, world) {
                    silos_now += 1;
                }
            }
            let has_ad = self.econ.buildings.iter().any(|b| b.kind == IDX_AIRDEF && b.country as usize == c && b.dismantle_at.is_none());
            if !has_ad {
                let inbound = self.flights.iter().any(|f| f.intercepted_at.is_none() && world.map.owner[f.tcell] as usize == c);
                let foe_silo = self.econ.buildings.iter().any(|b| b.kind == IDX_SILO && enemies.contains(&b.country) && b.dismantle_at.is_none());
                let cost = self.econ.build_cost(IDX_AIRDEF);
                if (inbound || foe_silo) && self.econ.can_afford(c, &cost) {
                    let chance = if inbound { 0.62 } else { 0.19 };
                    if self.rng.f() < chance {
                        let _ = self.econ_ai_place(c, IDX_AIRDEF, world);
                    }
                }
            }
        }
        if now - self.ai_missile_last_global < AIM_GLOBAL_GAP_S {
            return;
        }
        if self.flights.iter().any(|f| Some(f.country) != self.player && now < f.impact_at + IMPACT_SPREAD_S) {
            return;
        }
        let silos: Vec<(u32, u16)> = self.econ.buildings.iter().filter(|b| b.kind == IDX_SILO && b.dismantle_at.is_none() && Some(b.country) != self.player).map(|b| (b.id, b.country)).collect();
        if silos.is_empty() {
            return;
        }
        let start = self.rng.range(silos.len());
        for k in 0..silos.len() {
            let (sid, c) = silos[(start + k) % silos.len()];
            let ci = c as usize;
            let enemies = self.war_enemies(ci);
            if enemies.is_empty() || self.capital_fallen[ci] || !self.can_afford_missile(c) {
                continue;
            }
            if now - self.ai_missile_last_country[ci] < AIM_COUNTRY_GAP_S || self.silo_cooldown_left(sid) > 0.0 {
                continue;
            }
            if self.rng.f() >= AIM_LAUNCH * rate * boost {
                continue;
            }
            let Some((tx, ty)) = self.ai_missile_target(ci, &enemies, world) else { continue };
            if self.launch_missile(c, sid, tx, ty, world).is_ok() {
                self.ai_missile_last_global = now;
                self.ai_missile_last_country[ci] = now;
                break;
            }
        }
    }

    // ------------------------------------------------------------------ naval invasions
    pub fn naval_max_boats(&self, att: u16) -> u32 {
        (((self.manpower[att as usize] / NAVAL_MANPOWER_PER_BOAT).floor()) as u32).min(NAVAL_MAX_BOATS)
    }

    pub fn has_port(&self, map: &MapData, c: u16) -> bool {
        !self.econ.ports_of(map, c).is_empty()
    }

    /// A random coastal spot to put a port (self-test helper).
    pub fn econ_any_coastal_spot(&mut self, map: &MapData, country: u16) -> Option<(i32, i32)> {
        let ctx = EconCtx {
            names: &self.names,
            cells: &self.cells,
            orig: &self.orig_cells,
            fallen: &self.capital_fallen,
            eliminated: &self.eliminated,
            pop: &self.pop,
            gdp: &self.live_gdp,
            bbox: &self.bbox,
                mod_money: &self.mod_money,
                mod_prod: &self.mod_prod,
        };
        self.econ.any_coastal_spot(map, country, &ctx)
    }

    /// _navalPlan: BFS over water from the defender's coast back to one of the attacker's ports.
    fn naval_plan(&mut self, att: u16, def: u16, world: &World, manual_cell: Option<usize>) -> Option<Naval> {
        let (w, h) = (self.w as i32, self.h as i32);
        let wu = self.w;
        let n = wu * self.h;
        let map = &world.map;
        let land = &map.land;
        let owner = &map.owner;
        let manual = manual_cell.is_some();
        let ports = self.econ.ports_of(map, att);
        if ports.is_empty() {
            return None;
        }
        let mut port_water: HashMap<usize, u32> = HashMap::new();
        for &bi in &ports {
            let b = &self.econ.buildings[bi];
            for wc in self.econ.port_water_cells(map, b) {
                port_water.entry(wc).or_insert(b.id);
            }
        }
        if port_water.is_empty() {
            return None;
        }
        let coastal = |i: usize| -> bool {
            let (x, y) = ((i % wu) as i32, (i / wu) as i32);
            (x > 0 && land[i - 1] == 0) || (x < w - 1 && land[i + 1] == 0) || (y > 0 && land[i - wu] == 0) || (y < h - 1 && land[i + wu] == 0)
        };
        let bb = self.bbox[def as usize];
        let mut def_cells: Vec<usize> = Vec::new();
        if let Some(pc) = manual_cell {
            let (px, py) = ((pc % wu) as i64, (pc / wu) as i64);
            let mut best: Option<usize> = None;
            let mut bd = i64::MAX;
            for y in bb[1].max(0)..=bb[3].min(h - 1) {
                for x in bb[0].max(0)..=bb[2].min(w - 1) {
                    let i = (y * w + x) as usize;
                    if owner[i] != def || !coastal(i) {
                        continue;
                    }
                    let d = (x as i64 - px).pow(2) + (y as i64 - py).pow(2);
                    if d < bd {
                        bd = d;
                        best = Some(i);
                    }
                }
            }
            def_cells.push(best?);
        } else {
            for y in bb[1].max(0)..=bb[3].min(h - 1) {
                for x in bb[0].max(0)..=bb[2].min(w - 1) {
                    let i = (y * w + x) as usize;
                    if owner[i] == def && coastal(i) {
                        def_cells.push(i);
                    }
                }
            }
        }
        if self.sea_dist.len() != n {
            self.sea_dist = vec![-1; n];
            self.sea_queue = vec![0; n];
        }
        for v in self.sea_dist.iter_mut() {
            *v = -1;
        }
        let mut coast_owner: HashMap<usize, usize> = HashMap::new();
        let mut qt = 0usize;
        for &i in &def_cells {
            let (x, y) = ((i % wu) as i32, (i / wu) as i32);
            let mut nbs: Vec<usize> = Vec::with_capacity(4);
            if x > 0 { nbs.push(i - 1); }
            if x < w - 1 { nbs.push(i + 1); }
            if y > 0 { nbs.push(i - wu); }
            if y < h - 1 { nbs.push(i + wu); }
            for nb in nbs {
                if land[nb] == 0 && self.sea_dist[nb] == -1 {
                    self.sea_dist[nb] = 0;
                    coast_owner.insert(nb, i);
                    self.sea_queue[qt] = nb as u32;
                    qt += 1;
                }
            }
        }
        if qt == 0 {
            return None;
        }
        let max_d = if manual { i32::MAX } else { NAVAL_MAX_SEA_DIST };
        let budget = if manual { n } else { NAVAL_SEARCH_BUDGET };
        let mut start: Option<usize> = None;
        let mut port_id: Option<u32> = None;
        for i in 0..qt {
            let c = self.sea_queue[i] as usize;
            if let Some(&pid) = port_water.get(&c) {
                start = Some(c);
                port_id = Some(pid);
                break;
            }
        }
        let mut qh = 0usize;
        let mut scanned = 0usize;
        while start.is_none() && qh < qt && scanned < budget {
            let cur = self.sea_queue[qh] as usize;
            qh += 1;
            scanned += 1;
            let d = self.sea_dist[cur];
            if d >= max_d {
                continue;
            }
            let (cx, cy) = ((cur % wu) as i32, (cur / wu) as i32);
            for k in 0..4 {
                let (nx, ny) = (cx + if k == 0 { 1 } else if k == 1 { -1 } else { 0 }, cy + if k == 2 { 1 } else if k == 3 { -1 } else { 0 });
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * wu + nx as usize;
                if land[ni] == 1 || self.sea_dist[ni] != -1 {
                    continue;
                }
                self.sea_dist[ni] = d + 1;
                if let Some(&pid) = port_water.get(&ni) {
                    start = Some(ni);
                    port_id = Some(pid);
                    break;
                }
                if qt < n {
                    self.sea_queue[qt] = ni as u32;
                    qt += 1;
                }
            }
        }
        let start = start?;
        let pid = port_id?;
        let embark = {
            let pb = self.econ.buildings.iter().find(|b| b.id == pid)?;
            self.econ.port_anchor(pb)
        };
        let mut route: Vec<usize> = Vec::new();
        let mut cur = start;
        let mut guard = n + 4;
        loop {
            route.push(cur);
            let d = self.sea_dist[cur];
            if d <= 0 || guard == 0 {
                break;
            }
            guard -= 1;
            let (cx, cy) = ((cur % wu) as i32, (cur / wu) as i32);
            let mut next: Option<usize> = None;
            for k in 0..4 {
                let (nx, ny) = (cx + if k == 0 { 1 } else if k == 1 { -1 } else { 0 }, cy + if k == 2 { 1 } else if k == 3 { -1 } else { 0 });
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * wu + nx as usize;
                if land[ni] == 0 && self.sea_dist[ni] == d - 1 {
                    next = Some(ni);
                    break;
                }
            }
            match next {
                Some(nx) => cur = nx,
                None => break,
            }
        }
        if route.is_empty() {
            return None;
        }
        if route.len() == 1 {
            route.push(route[0]);
        }
        let last = *route.last()?;
        let target = *coast_owner.get(&last)?;
        if owner[target] != def {
            return None;
        }
        let mut path: Vec<usize> = Vec::new();
        let mut i = 0;
        while i < route.len() {
            path.push(route[i]);
            i += NAVAL_PATH_STRIDE;
        }
        if *path.last()? != last {
            path.push(last);
        }
        Some(Naval { embark, land: Some(target), path, len: route.len(), load: 0.0, boats: Vec::new(), pending: 0, manual, ai: !manual, port_id: Some(pid) })
    }

    /// _navalLand: put a beachhead of troops ashore.
    fn naval_land(&mut self, f: &mut Front, land: usize, world: &mut World) -> bool {
        if world.map.owner[land] != f.defender {
            return false;
        }
        let cost = self.cost_per_cell(f);
        let take = NAVAL_BEACHHEAD_CELLS.min((f.committed / cost).floor() as usize);
        if take == 0 {
            return false;
        }
        let (w, h) = (self.w as i32, self.h as i32);
        let wu = self.w;
        let mut pocket: Vec<usize> = Vec::new();
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut q: Vec<usize> = vec![land];
        seen.insert(land);
        let mut qi = 0;
        while qi < q.len() && pocket.len() < take {
            let c = q[qi];
            qi += 1;
            if world.map.owner[c] != f.defender {
                continue;
            }
            pocket.push(c);
            let (x, y) = ((c % wu) as i32, (c / wu) as i32);
            let mut nbs: Vec<usize> = Vec::with_capacity(4);
            if x < w - 1 { nbs.push(c + 1); }
            if x > 0 { nbs.push(c - 1); }
            if y < h - 1 { nbs.push(c + wu); }
            if y > 0 { nbs.push(c - wu); }
            for nb in nbs {
                if seen.insert(nb) && world.map.owner[nb] == f.defender {
                    q.push(nb);
                }
            }
        }
        if pocket.is_empty() {
            return false;
        }
        self.capture_pocket(f, &pocket, cost, world);
        let msg = format!("{} LANDS TROOPS IN {}", self.names[f.attacker as usize], self.names[f.defender as usize]);
        self.say(msg);
        true
    }

    /// _navalTick: load boats at the port, sail them along the route, land them. True while active.
    fn naval_tick(&mut self, f: &mut Front, dt: f32, world: &mut World) -> bool {
        let (att, def) = (f.attacker, f.defender);
        let human = self.is_human(att);
        if f.naval.is_none() && !human && f.committed > 0.0 && f.frontier.is_empty() && self.time - f.naval_replan_at >= NAVAL_REPLAN_S {
            f.naval_replan_at = self.time;
            if !self.econ.ports_of(&world.map, att).is_empty() {
                if let Some(mut plan) = self.naval_plan(att, def, world, None) {
                    plan.pending = 1;
                    f.naval = Some(plan);
                }
            }
        }
        let Some(mut nv) = f.naval.take() else { return false };
        // a boat is at sea: sail it
        if !nv.boats.is_empty() && nv.path.len() > 1 {
            nv.boats[0] = (nv.boats[0] + NAVAL_BOAT_CELLS_PER_SEC * dt / nv.len.max(1) as f32).min(1.0);
            if nv.boats[0] >= 1.0 {
                nv.boats.clear();
                let ok = match nv.land {
                    Some(l) => self.naval_land(f, l, world),
                    None => false,
                };
                if !ok {
                    nv.land = None;
                }
            }
            if f.done {
                return false;
            }
            f.naval = Some(nv);
            return true;
        }
        let usable = match nv.land {
            Some(l) => {
                nv.path.len() > 1
                    && world.map.owner[nv.embark] == att
                    && world.map.owner[l] == def
                    && nv.port_id.map_or(true, |pid| self.econ.ports_of(&world.map, att).iter().any(|&bi| self.econ.buildings[bi].id == pid))
            }
            None => false,
        };
        if !usable {
            if self.time - f.naval_replan_at < NAVAL_REPLAN_S {
                f.naval = Some(nv);
                return true;
            }
            f.naval_replan_at = self.time;
            let manual_cell = if nv.manual { nv.land } else { None };
            match self.naval_plan(att, def, world, manual_cell) {
                None => {
                    if human && self.econ.ports_of(&world.map, att).is_empty() {
                        let msg = format!("{} NEEDS A PORT TO REACH {}", self.names[att as usize], self.names[def as usize]);
                        self.say(msg);
                    }
                    f.naval = None;
                    return false;
                }
                Some(mut plan) => {
                    plan.manual = nv.manual;
                    plan.ai = !nv.manual;
                    plan.pending = nv.pending;
                    nv = plan;
                }
            }
        }
        if f.committed <= 0.0 {
            nv.load = 0.0;
            f.naval = Some(nv);
            return true;
        }
        nv.load = (nv.load + dt / NAVAL_LOAD_S).min(1.0);
        if nv.load >= 1.0 {
            nv.load = 0.0;
            if nv.pending > 0 {
                nv.pending -= 1;
                nv.boats.push(0.0);
            }
        }
        if nv.pending == 0 && nv.boats.is_empty() {
            f.naval = None;
            return false;
        }
        f.naval = Some(nv);
        true
    }

    /// launchNavalInvasion: send `boats` boats (4000 troops each) to land at `cell` inside `def`.
    pub fn launch_naval(&mut self, att: u16, def: u16, cell: usize, boats: u32, world: &World) -> Result<(), &'static str> {
        if att == def || !self.is_at_war(att, def) {
            return Err("YOU ARE NOT AT WAR WITH THEM");
        }
        let map = &world.map;
        if cell >= map.owner.len() || map.land[cell] != 1 || map.owner[cell] != def {
            return Err("PICK A LANDING SPOT INSIDE THEIR LAND");
        }
        let n = boats.clamp(1, NAVAL_MAX_BOATS);
        let pool = self.manpower[att as usize];
        let floor = NAVAL_MANPOWER_PER_BOAT * n as f32;
        if pool < floor {
            return Err("NOT ENOUGH MANPOWER FOR THOSE BOATS");
        }
        if self.econ.ports_of(map, att).is_empty() {
            return Err("YOU NEED A PORT TO REACH THEM");
        }
        let Some(mut plan) = self.naval_plan(att, def, world, Some(cell)) else { return Err("NO WATER PATH TO THAT SHORE") };
        if self.commit(att, def, floor) <= 0.0 {
            return Err("NOT ENOUGH MANPOWER");
        }
        let Some(fi) = self.fronts.iter().position(|f| f.attacker == att && f.defender == def && !f.done) else { return Err("NO FRONT") };
        let f = &mut self.fronts[fi];
        match &mut f.naval {
            Some(old) if !old.boats.is_empty() => {
                old.manual = true;
                old.pending += n;
            }
            other => {
                plan.manual = true;
                plan.pending = other.as_ref().map_or(0, |o| o.pending) + n;
                *other = Some(plan);
            }
        }
        let msg = format!("{} SENDS {} BOAT{} TO {}", self.names[att as usize], n, if n > 1 { "S" } else { "" }, self.names[def as usize]);
        self.say(msg);
        Ok(())
    }

    /// Pull the troops back from the front against `def` (the war itself stays on).
    pub fn stop_attack(&mut self, att: u16, def: u16) {
        for k in 0..self.fronts.len() {
            if self.fronts[k].attacker == att && self.fronts[k].defender == def && !self.fronts[k].done {
                self.return_front(k);
            }
        }
    }

    /// A random free spot where `country` may build (self-test helper).
    pub fn econ_any_spot(&mut self, map: &MapData, country: u16) -> Option<(i32, i32)> {
        let ctx = EconCtx {
            names: &self.names,
            cells: &self.cells,
            orig: &self.orig_cells,
            fallen: &self.capital_fallen,
            eliminated: &self.eliminated,
            pop: &self.pop,
            gdp: &self.gdp,
            bbox: &self.bbox,
                mod_money: &self.mod_money,
                mod_prod: &self.mod_prod,
        };
        self.econ.any_spot(map, country, &ctx)
    }

    pub fn decline_offer(&mut self, from: u16) {
        self.peace_offers.retain(|&x| x != from);
        self.offer_cd[from as usize] = self.time + 15.0;
    }

    /// Player asks `other` for peace. Returns true if it was accepted.
    pub fn propose_peace(&mut self, player: u16, other: u16) -> bool {
        if !self.is_at_war(player, other) {
            return false;
        }
        let accept = self.ai_wants_peace(other, player) || self.no_front_between(player, other);
        if accept {
            self.make_peace(player, other);
            self.say("PEACE TREATY SIGNED".to_string());
            true
        } else {
            self.say("PEACE OFFER REFUSED".to_string());
            false
        }
    }

    fn no_front_between(&self, a: u16, b: u16) -> bool {
        !self.fronts.iter().any(|f| !f.done && ((f.attacker == a && f.defender == b) || (f.attacker == b && f.defender == a)))
    }

    /// _aiWantsPeace(defender, attacker, fracLost)
    fn ai_wants_peace(&self, defender: u16, attacker: u16) -> bool {
        let d = defender as usize;
        let frac_lost = clamp01(1.0 - self.cells[d] as f32 / self.orig_cells[d].max(1) as f32);
        let def_mp = self.manpower[d];
        let att_mp = self.manpower[attacker as usize].max(1.0);
        let mp_ratio = clamp01(1.0 - def_mp / (def_mp + att_mp));
        let fallen = if self.capital_fallen[d] { PEACE_LOSS_FRAC_BONUS } else { 0.0 };
        let pressure = frac_lost * AI_ACCEPT_LOSS_W + mp_ratio * AI_MANPOWER_RATIO_W + fallen;
        let resolve = self.aggression[d] * 0.7;
        pressure > resolve
    }

    // ------------------------------------------------------------------ main loop
    pub fn step(&mut self, real_dt: f32, world: &mut World) {
        if self.paused {
            return;
        }
        let dt = real_dt.min(0.1) * self.speed;
        self.acc += dt;
        if self.acc < SIM_STEP {
            return;
        }
        let sd = self.acc.min(0.25);
        self.acc = 0.0;
        self.tick += 1;
        self.time += sd;
        self.game_days += sd; // REAL_SECONDS_PER_GAME_DAY = 1
        if !self.pending_settle.is_empty() {
            let pend = std::mem::take(&mut self.pending_settle);
            for (a, b) in pend {
                self.settle_cells(a, b, false, world);
            }
        }

        self.adj_timer += sd;
        if self.adj_timer >= 1.0 {
            self.adj_timer = 0.0;
            self.rebuild_adjacency(world);
        }
        self.update_defense_allocs(sd);
        self.update_front_supply(sd, &*world);
        for k in 0..self.fronts.len() {
            if !self.fronts[k].done {
                self.advance(k, sd, world);
            }
        }
        self.check_capitals(world);
        for k in 0..self.fronts.len() {
            if self.fronts[k].done && self.fronts[k].committed > 0.0 {
                self.return_front(k);
            }
        }
        for k in 0..self.fronts.len() {
            if self.fronts[k].done {
                self.credit_battle_outcome(k);
            }
        }
        self.fronts.retain(|f| !f.done);
        if self.tick & 63 == 0 && self.recent.len() > 4000 {
            let cut = self.tick.saturating_sub(RECAPTURE_HOLD_TICKS * 4);
            self.recent.retain(|_, v| *v >= cut);
        }

        self.ai_timer += sd;
        if self.ai_timer >= AI_PASS_S {
            self.ai_timer = 0.0;
            self.ai_pass(&*world);
            self.annex_pass(world);
        }
        self.empire_ai_timer += sd;
        if self.empire_ai_timer >= ideology::EMPIRE_AI_CHECK_S {
            self.empire_ai_timer = 0.0;
            self.empire_ai_pass(&*world);
        }
        self.peace_timer += sd;
        if self.peace_timer >= 2.0 {
            let t = self.peace_timer;
            self.peace_timer = 0.0;
            self.peace_pass(t, &*world);
        }
        self.regen(sd);
        self.update_population(sd);
        self.troop_food(sd);
        {
            let ctx = EconCtx {
                names: &self.names,
                cells: &self.cells,
                orig: &self.orig_cells,
                fallen: &self.capital_fallen,
                eliminated: &self.eliminated,
                pop: &self.pop,
                gdp: &self.live_gdp,
                bbox: &self.bbox,
                mod_money: &self.mod_money,
                mod_prod: &self.mod_prod,
            };
            self.econ.step(sd, &ctx, &world.map, self.player, self.time);
        }
        self.tick_missiles(world);
        let now = self.time;
        self.log.retain(|(_, t)| now - *t < 30.0);
    }

    // ------------------------------------------------------------------ victory points (crowns) + annexation
    pub fn crowns_of(&self, id: usize) -> u32 {
        self.crowns[id]
    }

    /// _awardVictoryPoints: integer only, monotonic, clamped.
    fn award_crowns(&mut self, id: u16, pts: u32) {
        let i = id as usize;
        if i == 0 || i >= self.n || pts == 0 {
            return;
        }
        self.crowns[i] = (self.crowns[i] + pts).min(VP_MAX_POINTS);
    }

    /// _addVpProgress: fractional -> whole-point bank (key 0 = occupied cells, 1 = defensive kills).
    fn add_vp_progress(&mut self, id: u16, key: usize, amount: f32, per_point: f32) {
        let i = id as usize;
        if i == 0 || i >= self.n || !(amount > 0.0) || !(per_point > 0.0) {
            return;
        }
        let total = self.vp_progress[i][key] + amount;
        let pts = (total / per_point).floor();
        self.vp_progress[i][key] = total - pts * per_point;
        if pts >= 1.0 {
            self.award_crowns(id, pts as u32);
        }
    }

    /// _creditOccupation: called where cells flip mid-war (frontier push and enclave mop-up).
    fn credit_occupation(&mut self, f: &mut Front, cells: usize) {
        if cells == 0 {
            return;
        }
        f.vp_cells += cells as f32;
        self.add_vp_progress(f.attacker, 0, cells as f32, VP_OCCUPY_CELLS_PER_POINT);
    }

    /// _creditBattleOutcome: once per front, when it ends.
    fn credit_battle_outcome(&mut self, k: usize) {
        if self.fronts[k].vp_settled {
            return;
        }
        self.fronts[k].vp_settled = true;
        let (a, d, cells) = (self.fronts[k].attacker, self.fronts[k].defender, self.fronts[k].vp_cells);
        if cells >= VP_BATTLE_WIN_MIN_CELLS {
            self.award_crowns(a, VP_BATTLE_WIN_POINTS);
        } else if cells <= VP_BATTLE_HELD_MAX_CELLS {
            self.award_crowns(d, VP_BATTLE_WIN_POINTS);
        }
    }

    /// fullAnnexCost: crowns to keep an entire nation, from its own stats.
    pub fn full_annex_cost(&self, id: usize) -> u32 {
        let pop = self.pop_base[id].max(0.01);
        let gdp = self.gdp[id].max(1.0);
        let raw = ANNEX_POP_WEIGHT * pop.powf(ANNEX_POP_EXP) + ANNEX_GDP_WEIGHT * gdp.powf(ANNEX_GDP_EXP);
        (raw.round() as u32).clamp(ANNEX_MIN_FULL_COST, ANNEX_MAX_FULL_COST)
    }

    /// annexCostCells: pro-rata price of `cells` of `loser`.
    pub fn annex_cost_cells(&self, loser: usize, cells: u32) -> u32 {
        if cells == 0 {
            return 0;
        }
        let full = self.full_annex_cost(loser);
        let total = self.orig_cells[loser].max(1) as f32;
        let frac = (cells as f32 / total).clamp(0.0, 1.0);
        ((full as f32 * frac).ceil() as u32).clamp(1, full)
    }

    fn bbox_add(bx: &mut [i32; 4], i: usize, w: usize) {
        let (x, y) = ((i % w) as i32, (i / w) as i32);
        bx[0] = bx[0].min(x);
        bx[1] = bx[1].min(y);
        bx[2] = bx[2].max(x);
        bx[3] = bx[3].max(y);
    }

    /// settlePeace (territory part): peace keeps exactly what each side physically occupies and makes it
    /// permanent. `auto` = full-occupation settlement: the victor also takes every cell the fallen defender
    /// still holds (unless it is also being fought by someone else), so it vanishes instead of surviving as a strip.
    /// _cleanPockets: before a settlement is drawn, absorb any isolated cell of one side (or a tiny
    /// dimple) that is effectively swallowed by the other -- an 8-neighbour majority rule, skipping
    /// capitals, run for `passes` iterations or until nothing flips.
    fn clean_pockets(&mut self, a: u16, b: u16, world: &mut World, passes: u32) {
        let (gw, gh) = (world.map.w as i32, world.map.h as i32);
        let (cap_a, cap_b) = (self.capital[a as usize], self.capital[b as usize]);
        for _ in 0..passes {
            let mut flips: Vec<(usize, u16)> = Vec::new();
            for i in 0..world.map.owner.len() {
                let cur = world.map.owner[i];
                if cur != a && cur != b {
                    continue;
                }
                if Some(i) == cap_a || Some(i) == cap_b {
                    continue;
                }
                let other = if cur == a { b } else { a };
                let (x, y) = ((i % gw as usize) as i32, (i / gw as usize) as i32);
                let (mut same, mut opp, mut edge) = (0, 0, 0);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= gw || ny >= gh {
                            edge += 1;
                            continue;
                        }
                        let v = world.map.owner[ny as usize * gw as usize + nx as usize];
                        if v == cur {
                            same += 1;
                        } else if v == other {
                            opp += 1;
                        }
                    }
                }
                if opp >= 6 && same <= 1 + edge {
                    flips.push((i, other));
                }
            }
            if flips.is_empty() {
                break;
            }
            for (i, other) in flips {
                let from = world.map.owner[i] as usize;
                world.set_owner(i, other);
                self.cells[from] = self.cells[from].saturating_sub(1);
                self.cells[other as usize] += 1;
                self.dirty.push([ (i % gw as usize) as i32, (i / gw as usize) as i32, (i % gw as usize) as i32, (i / gw as usize) as i32 ]);
            }
        }
    }

    /// _naturalizeBorder: the frontline a war leaves behind is a dead-straight ruler line. Erode
    /// protrusions and roughen straight seams with deterministic noise so the settled border looks
    /// like a negotiated historical one. Unlike clean_pockets this also moves the original owner
    /// (set_orig), making the new line permanent rather than "occupied" land.
    fn naturalize_border(&mut self, a: u16, b: u16, world: &mut World, passes: u32) {
        let (gw, gh) = (world.map.w as i32, world.map.h as i32);
        let (cap_a, cap_b) = (self.capital[a as usize], self.capital[b as usize]);
        let seed_base = (self.tick & 0xffff_ffff) as i32;
        for p in 0..passes {
            let mut flips: Vec<(usize, u16)> = Vec::new();
            for i in 0..world.map.owner.len() {
                let cur = world.map.owner[i];
                if cur != a && cur != b {
                    continue;
                }
                if Some(i) == cap_a || Some(i) == cap_b {
                    continue;
                }
                let other = if cur == a { b } else { a };
                let (x, y) = ((i % gw as usize) as i32, (i / gw as usize) as i32);
                let (mut same, mut opp) = (0, 0);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= gw || ny >= gh {
                            continue;
                        }
                        let v = world.map.owner[ny as usize * gw as usize + nx as usize];
                        if v == cur {
                            same += 1;
                        } else if v == other {
                            opp += 1;
                        }
                    }
                }
                if opp == 0 {
                    continue;
                }
                let h = nuke_noise(i as i32, 0, seed_base + p as i32);
                let flip = if opp > same {
                    h < 0.85
                } else {
                    opp >= 3 && same >= 4 && h < 0.30
                };
                if flip {
                    flips.push((i, other));
                }
            }
            if flips.is_empty() {
                break;
            }
            for (i, other) in flips {
                let from = world.map.owner[i] as usize;
                world.set_owner(i, other);
                world.set_orig(i, other);
                self.cells[from] = self.cells[from].saturating_sub(1);
                self.cells[other as usize] += 1;
                self.dirty.push([ (i % gw as usize) as i32, (i / gw as usize) as i32, (i % gw as usize) as i32, (i / gw as usize) as i32 ]);
            }
        }
    }

    fn settle_cells(&mut self, a: u16, b: u16, auto: bool, world: &mut World) {
        let (ai, bi) = (a as usize, b as usize);
        let other_war = auto && (1..self.n).any(|o| o != ai && o != bi && self.at_war[bi * self.n + o]);
        let take_rest = auto && !other_war;
        let mut gained_a: Vec<usize> = Vec::new();
        let mut gained_b: Vec<usize> = Vec::new();
        let mut rest: Vec<usize> = Vec::new();
        {
            let own = &world.map.owner;
            let orig = &world.orig_owner;
            for i in 0..own.len() {
                let (cur, o) = (own[i], orig[i]);
                if cur == a && o == b {
                    gained_a.push(i);
                } else if cur == b && o == a && !auto {
                    gained_b.push(i);
                } else if take_rest && cur == b {
                    rest.push(i);
                }
            }
        }
        let w = self.w;
        let mut bx = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        for &i in &gained_a {
            world.set_orig(i, a);
            Self::bbox_add(&mut bx, i, w);
        }
        for &i in &gained_b {
            world.set_orig(i, b);
            Self::bbox_add(&mut bx, i, w);
        }
        let mut abx = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        for &i in &rest {
            world.set_owner(i, a);
            world.set_orig(i, a);
            Self::bbox_add(&mut bx, i, w);
            Self::bbox_add(&mut abx, i, w);
        }
        if !rest.is_empty() {
            let nr = rest.len() as u32;
            self.cells[bi] = self.cells[bi].saturating_sub(nr);
            self.cells[ai] += nr;
            let ab = &mut self.bbox[ai];
            ab[0] = ab[0].min(abx[0]);
            ab[1] = ab[1].min(abx[1]);
            ab[2] = ab[2].max(abx[2]);
            ab[3] = ab[3].max(abx[3]);
        }
        if bx[2] >= bx[0] {
            self.dirty.push(bx);
        }
        let na = gained_a.len() + rest.len();
        if na > 0 {
            self.stat_annexed += na as u32;
            let msg = format!("{} ANNEXED {} OCCUPIED CELLS FROM {}", self.names[ai], na, self.names[bi]);
            self.say(msg);
        }
        if !gained_b.is_empty() {
            self.stat_annexed += gained_b.len() as u32;
            let msg = format!("{} ANNEXED {} OCCUPIED CELLS FROM {}", self.names[bi], gained_b.len(), self.names[ai]);
            self.say(msg);
        }
        if take_rest {
            let msg = format!("{} FULLY ANNEXED BY {}", self.names[bi], self.names[ai]);
            self.say(msg);
        }
        self.clean_pockets(a, b, world, 2);
        if !auto {
            self.naturalize_border(a, b, world, 2);
        }
        if self.cells[bi] == 0 && !self.eliminated[bi] {
            self.eliminate(b);
        }
    }

    /// Full-occupation rule (the first part of the JS AI pass, applied to every nation at war): a country that has
    /// lost FULL_OCCUPATION_FRAC of its original land is simply over. Every attacker holding a meaningful share
    /// annexes its slice; if nobody clears the bar the largest holder settles.
    fn annex_pass(&mut self, world: &mut World) {
        let n = self.n;
        for d in 1..n {
            if self.eliminated[d] || !(1..n).any(|o| self.at_war[d * n + o]) {
                continue;
            }
            let total = self.orig_cells[d].max(1) as f32;
            let remaining = world.core[d] as f32;
            if remaining > 0.0 && (total - remaining) / total < FULL_OCCUPATION_FRAC {
                continue;
            }
            let shares: Vec<(usize, u32)> = (1..n).filter(|&a| self.at_war[d * n + a]).map(|a| (a, world.held(a as u16, d as u16))).collect();
            let mut settlers: Vec<usize> = shares.iter().filter(|&&(_, c)| c as f32 / total >= AUTO_ANNEX_MIN_ATTACKER_FRAC).map(|&(a, _)| a).collect();
            if settlers.is_empty() {
                if let Some(&(a, _)) = shares.iter().max_by_key(|&&(_, c)| c) {
                    settlers.push(a);
                }
            }
            for a in settlers {
                if self.eliminated[d] || !self.at_war[a * n + d] {
                    continue;
                }
                self.end_war(a as u16, d as u16);
                self.settle_cells(a as u16, d as u16, true, world);
            }
        }
    }

    // ------------------------------------------------------------------ regimes + policy (ideology.rs)
    fn refresh_mods(&mut self, id: usize) {
        let mut m = ideology::mods_for(self.ideology[id], &self.policy[id]);
        if self.empire[id] {
            m = ideology::apply_empire_perks(m, &self.empire_perks[id]);
        }
        self.mod_money[id] = m[ideology::K_MONEY];
        self.mod_prod[id] = m[ideology::K_PROD];
        self.mods[id] = m;
    }

    /// _spendVictoryPoints: integer crowns, all-or-nothing.
    fn spend_crowns(&mut self, id: usize, amount: u32) -> bool {
        if amount == 0 {
            return true;
        }
        if self.crowns[id] < amount {
            return false;
        }
        self.crowns[id] -= amount;
        true
    }

    pub fn is_empire(&self, id: usize) -> bool {
        self.empire[id]
    }

    /// canFormEmpire: same {ok,reason}-shape check as can_switch_ideology.
    pub fn can_form_empire(&self, id: usize, world: &World) -> Result<(), String> {
        if self.empire[id] {
            return Err("ALREADY AN EMPIRE".to_string());
        }
        let vp = self.crowns[id];
        if vp < ideology::EMPIRE_REQUIRE_VP {
            return Err(format!("NEED {} CROWNS ({})", ideology::EMPIRE_REQUIRE_VP, vp));
        }
        if self.manpower[id] < ideology::EMPIRE_REQUIRE_MANPOWER {
            return Err(format!("NEED {:.0} MANPOWER", ideology::EMPIRE_REQUIRE_MANPOWER));
        }
        let foreign = world.foreign_held(id as u16);
        if foreign < ideology::EMPIRE_REQUIRE_FOREIGN_CELLS {
            return Err(format!("NEED MORE FOREIGN LAND HELD ({}/{})", foreign, ideology::EMPIRE_REQUIRE_FOREIGN_CELLS));
        }
        if vp < ideology::EMPIRE_REQUIRE_VP + ideology::EMPIRE_FORM_VP_COST {
            return Err(format!("NEED {} CROWNS TO DECLARE", ideology::EMPIRE_REQUIRE_VP + ideology::EMPIRE_FORM_VP_COST));
        }
        Ok(())
    }

    /// formEmpire: founds with zero free perks picked (perks are bought afterward via buy_empire_perk,
    /// matching the shop-price path but letting the player pick deliberately rather than at a rushed
    /// founding moment).
    pub fn form_empire(&mut self, id: usize, world: &World) -> bool {
        if self.can_form_empire(id, world).is_err() || !self.spend_crowns(id, ideology::EMPIRE_FORM_VP_COST) {
            return false;
        }
        self.empire[id] = true;
        self.refresh_mods(id);
        let msg = format!("{} HAS RISEN", self.empire_name[id]);
        self.say(msg);
        true
    }

    fn appearance_cost(&self, id: usize) -> u32 {
        (ideology::REPUBLIC_SAVE_BASE_COST + self.cells[id] as f32 / 300.0 * ideology::REPUBLIC_SAVE_COST_PER_300_CELLS).round() as u32
    }

    /// revertEmpire: dissolving is a real decision with a real price (double the appearance-save formula).
    pub fn revert_empire(&mut self, id: usize) -> bool {
        if !self.empire[id] {
            return false;
        }
        let cost = self.appearance_cost(id) * 2;
        if !self.spend_crowns(id, cost) {
            return false;
        }
        self.empire[id] = false;
        self.empire_perks[id].clear();
        self.refresh_mods(id);
        let msg = format!("{} REVERTED TO A REPUBLIC (-{} CROWNS)", self.names[id], cost);
        self.say(msg);
        true
    }

    /// buyEmpirePerk: price is always re-derived from the PERKS table, never cached, so it can't be undercut.
    pub fn buy_empire_perk(&mut self, id: usize, perk: usize) -> bool {
        if !self.empire[id] || perk >= ideology::PERKS.len() {
            return false;
        }
        if self.empire_perks[id].contains(&(perk as u8)) || self.empire_perks[id].len() >= ideology::EMPIRE_MAX_ACTIVE_PERKS {
            return false;
        }
        let def = &ideology::PERKS[perk];
        let cost = (def.cost as f32 * ideology::EMPIRE_SHOP_PRICE_MULT).round() as u32;
        if !self.spend_crowns(id, cost) {
            return false;
        }
        self.empire_perks[id].push(perk as u8);
        let msg = format!("{} PURCHASED {} (-{} CROWNS)", self.names[id], def.name, cost);
        self.say(msg);
        self.refresh_mods(id);
        true
    }

    /// AI nations that clear the bar periodically roll to found their own empire (CONFIG.empire.AI_CHECK_INTERVAL_MS).
    fn empire_ai_pass(&mut self, world: &World) {
        for id in 1..self.n {
            if self.is_human(id as u16) || self.eliminated[id] || self.empire[id] {
                continue;
            }
            if self.can_form_empire(id, world).is_err() {
                continue;
            }
            let r = ideology::hash01(ideology::hash_str(&format!("{}::empireRoll::{}", self.names[id], self.tick)), 0);
            if r < ideology::EMPIRE_AI_FORM_CHANCE {
                self.form_empire(id, world);
            }
        }
    }

    pub fn regime_def(&self, id: usize) -> &'static ideology::Regime {
        &ideology::REGIMES[self.ideology[id]]
    }

    pub fn mods_of(&self, id: usize) -> ideology::Mods {
        self.mods[id]
    }

    /// ideologyCooldownDaysLeft
    pub fn ideology_cooldown_left(&self, id: usize) -> f32 {
        (ideology::COOLDOWN_DAYS - (self.game_days - self.ideology_changed[id])).max(0.0)
    }

    /// ideologySwitchCost (money only): a slice of the nation's GDP, pricier mid-war.
    pub fn ideology_switch_cost(&self, id: usize, regime: usize) -> f32 {
        let at_war = self.fronts.iter().any(|f| !f.done && (f.attacker as usize == id || f.defender as usize == id));
        let g = self.gdp[id];
        let gdp = if g > 0.0 { g } else { 1.0 };
        gdp * ideology::SWITCH_MONEY_GDP_FRAC * ideology::REGIMES[regime].cost_mult * if at_war { ideology::UNREST_WAR_MULT } else { 1.0 }
    }

    /// canSwitchIdeology: shared by the action and the (advisory) button state.
    pub fn can_switch_ideology(&self, id: usize, regime: usize) -> Result<(), String> {
        if id == 0 || id >= self.n || self.eliminated[id] {
            return Err("NO NATION".to_string());
        }
        if regime >= ideology::REGIMES.len() {
            return Err("UNKNOWN REGIME".to_string());
        }
        if self.ideology[id] == regime {
            return Err("ALREADY ACTIVE".to_string());
        }
        let wait = self.ideology_cooldown_left(id);
        if wait > 0.0 {
            return Err(format!("WAIT {}D", wait.ceil() as u32));
        }
        if self.econ.stock[id][0] < self.ideology_switch_cost(id, regime) {
            return Err("NEED FUNDS".to_string());
        }
        Ok(())
    }

    /// setIdeology: pay, install the regime and its default slider platform.
    pub fn set_ideology(&mut self, id: usize, regime: usize) -> bool {
        if let Err(reason) = self.can_switch_ideology(id, regime) {
            self.say(reason);
            return false;
        }
        let cost = self.ideology_switch_cost(id, regime);
        self.econ.stock[id][0] -= cost;
        self.ideology[id] = regime;
        self.ideology_changed[id] = self.game_days;
        self.policy[id] = ideology::regime_policy(regime);
        self.refresh_mods(id);
        let msg = format!("{} IS NOW {}", self.names[id], ideology::REGIMES[regime].name.to_uppercase());
        self.say(msg);
        true
    }

    /// Test hook (CW_UITEST): install a regime without paying or waiting out the cooldown.
    pub fn force_regime(&mut self, id: usize, regime: usize) {
        if id == 0 || id >= self.n || regime >= ideology::REGIMES.len() {
            return;
        }
        self.ideology[id] = regime;
        self.policy[id] = ideology::regime_policy(regime);
        self.refresh_mods(id);
    }

    /// setPolicy: move one slider (snapped); returns false when nothing changed.
    pub fn set_policy(&mut self, id: usize, slider: usize, value: f32) -> bool {
        if id == 0 || id >= self.n || slider >= ideology::SLIDERS.len() || !value.is_finite() {
            return false;
        }
        let v = ideology::snap(&ideology::SLIDERS[slider], value);
        if (self.policy[id][slider] - v).abs() < 1e-6 {
            return false;
        }
        self.policy[id][slider] = v;
        self.refresh_mods(id);
        true
    }

    // ------------------------------------------------------------------ supply (CONFIG.supply)
    /// _supplyReachable: the land a nation can keep supplied. Flood from the capital over continuously owned
    /// cells (if the capital is occupied but the nation lives on, from the owned cell nearest to it). Once a
    /// home port is reached, every other owned port is a seed too (sea supply), so islands stay connected.
    fn compute_supply_reach(&mut self, id: usize, world: &World) -> Vec<u8> {
        let owner = &world.map.owner;
        let n = owner.len();
        let mut reached = self.supply_pool.pop().unwrap_or_default();
        if reached.len() == n {
            for v in reached.iter_mut() {
                *v = 0;
            }
        } else {
            reached.clear();
            reached.resize(n, 0);
        }
        if id == 0 || id >= self.n || self.cells[id] == 0 || self.eliminated[id] || self.capital_fallen[id] {
            return reached;
        }
        let idu = id as u16;
        let (w, h) = (self.w, self.h);
        let mut cap = self.capital[id];
        if cap.map_or(true, |c| owner[c] != idu) {
            // _capitalResistanceSupplyCell: owned cell nearest to the (lost) capital
            let lost = cap;
            cap = None;
            if let Some(c0) = lost {
                let (cx, cy) = ((c0 % w) as i64, (c0 / w) as i64);
                let b = self.bbox[id];
                let mut best = i64::MAX;
                for y in b[1].max(0)..=b[3].min(h as i32 - 1) {
                    for x in b[0].max(0)..=b[2].min(w as i32 - 1) {
                        let i = y as usize * w + x as usize;
                        if owner[i] == idu {
                            let (dx, dy) = (x as i64 - cx, y as i64 - cy);
                            let d = dx * dx + dy * dy;
                            if d < best {
                                best = d;
                                cap = Some(i);
                            }
                        }
                    }
                }
            }
        }
        let Some(cap) = cap else { return reached };
        let mut queue = std::mem::take(&mut self.supply_queue);
        queue.clear();
        reached[cap] = 1;
        queue.push(cap as u32);
        Self::supply_flood(owner, idu, w, h, &mut reached, &mut queue);
        let ports: Vec<usize> = self
            .econ
            .ports_of(&world.map, idu)
            .into_iter()
            .map(|i| self.econ.port_anchor(&self.econ.buildings[i]))
            .collect();
        if ports.iter().any(|&p| reached[p] != 0) {
            for &p in &ports {
                if owner[p] == idu && reached[p] == 0 {
                    reached[p] = 1;
                    queue.push(p as u32);
                }
            }
            Self::supply_flood(owner, idu, w, h, &mut reached, &mut queue);
        }
        self.supply_queue = queue;
        reached
    }

    /// 4-neighbour flood over cells owned by `idu`; `queue` holds the already-marked seeds.
    fn supply_flood(owner: &[u16], idu: u16, w: usize, h: usize, reached: &mut [u8], queue: &mut Vec<u32>) {
        let mut head = 0usize;
        while head < queue.len() {
            let i = queue[head] as usize;
            head += 1;
            let (x, y) = (i % w, i / w);
            if x + 1 < w && owner[i + 1] == idu && reached[i + 1] == 0 {
                reached[i + 1] = 1;
                queue.push((i + 1) as u32);
            }
            if x > 0 && owner[i - 1] == idu && reached[i - 1] == 0 {
                reached[i - 1] = 1;
                queue.push((i - 1) as u32);
            }
            if y + 1 < h && owner[i + w] == idu && reached[i + w] == 0 {
                reached[i + w] = 1;
                queue.push((i + w) as u32);
            }
            if y > 0 && owner[i - w] == idu && reached[i - w] == 0 {
                reached[i - w] = 1;
                queue.push((i - w) as u32);
            }
        }
        queue.clear();
    }

    /// _frontSupplyFraction: share of the active border that is connected to the nation's supply graph.
    /// A front with no land border yet (naval force still at sea) counts as fully supplied by its transport.
    fn front_supply_fraction(&self, f: &Front, att_side: bool, reach: &[u8], owner: &[u16]) -> f32 {
        let (w, h) = (self.w, self.h);
        let (own, enemy) = if att_side { (f.attacker, f.defender) } else { (f.defender, f.attacker) };
        if own == 0 || enemy == 0 {
            return 1.0;
        }
        let (mut total, mut supplied) = (0u32, 0u32);
        if att_side {
            let mut seen: Vec<usize> = Vec::new();
            for &di in &f.frontier {
                if owner[di] != f.defender {
                    continue;
                }
                let (x, y) = (di % w, di / w);
                if x + 1 < w && owner[di + 1] == own {
                    seen.push(di + 1);
                }
                if x > 0 && owner[di - 1] == own {
                    seen.push(di - 1);
                }
                if y + 1 < h && owner[di + w] == own {
                    seen.push(di + w);
                }
                if y > 0 && owner[di - w] == own {
                    seen.push(di - w);
                }
            }
            seen.sort_unstable();
            seen.dedup();
            for ni in seen {
                total += 1;
                if reach[ni] != 0 {
                    supplied += 1;
                }
            }
        } else {
            for &di in &f.frontier {
                if owner[di] != f.defender {
                    continue;
                }
                let (x, y) = (di % w, di / w);
                let touches = (x + 1 < w && owner[di + 1] == enemy)
                    || (x > 0 && owner[di - 1] == enemy)
                    || (y + 1 < h && owner[di + w] == enemy)
                    || (y > 0 && owner[di - w] == enemy);
                if touches {
                    total += 1;
                    if reach[di] != 0 {
                        supplied += 1;
                    }
                }
            }
        }
        if total > 0 {
            supplied as f32 / total as f32
        } else {
            1.0
        }
    }

    fn supply_step(cur: f32, target: f32, days: f32) -> f32 {
        let span = 1.0 - SUPPLY_MIN_READINESS;
        let rate = span / (if target < cur { SUPPLY_EXHAUST_DAYS } else { SUPPLY_RECOVER_DAYS }).max(0.001);
        let mv = (rate * days).max(0.0);
        (cur + (target - cur).clamp(-mv, mv)).clamp(SUPPLY_MIN_READINESS, 1.0)
    }

    /// _updateFrontSupply: topology is re-checked every SUPPLY_RECHECK_TICKS ticks; readiness integrates every
    /// tick in game days, and an isolated side bleeds troops allocated to that front.
    fn update_front_supply(&mut self, days: f32, world: &World) {
        let due = self.supply_last_check == u64::MAX
            || self.tick < self.supply_last_check
            || self.tick - self.supply_last_check >= SUPPLY_RECHECK_TICKS;
        if due {
            let old: Vec<Vec<u8>> = self.supply_reach.drain().map(|(_, v)| v).collect();
            for v in old {
                if self.supply_pool.len() < 8 {
                    self.supply_pool.push(v);
                }
            }
            self.supply_last_check = self.tick;
        }
        let owner = &world.map.owner;
        let mut notes: Vec<String> = Vec::new();
        for k in 0..self.fronts.len() {
            if self.fronts[k].done {
                continue;
            }
            let (a, d) = (self.fronts[k].attacker, self.fronts[k].defender);
            if due {
                for c in [a, d] {
                    if !self.supply_reach.contains_key(&c) {
                        let r = self.compute_supply_reach(c as usize, world);
                        self.supply_reach.insert(c, r);
                    }
                }
                let af = self.front_supply_fraction(&self.fronts[k], true, &self.supply_reach[&a], owner);
                let df = self.front_supply_fraction(&self.fronts[k], false, &self.supply_reach[&d], owner);
                let f = &mut self.fronts[k];
                f.att_supply_target = SUPPLY_MIN_READINESS + (1.0 - SUPPLY_MIN_READINESS) * af;
                f.def_supply_target = SUPPLY_MIN_READINESS + (1.0 - SUPPLY_MIN_READINESS) * df;
            }
            let (att, def, att_warn, att_ok, def_warn, def_ok) = {
                let f = &mut self.fronts[k];
                f.att_supply = Self::supply_step(f.att_supply, f.att_supply_target, days);
                f.def_supply = Self::supply_step(f.def_supply, f.def_supply_target, days);
                let (att, def) = (f.att_supply, f.def_supply);
                let att_warn = att < SUPPLY_WARNING_LEVEL && !f.att_supply_warned;
                let att_ok = att > SUPPLY_RESTORED_LEVEL && f.att_supply_warned;
                let def_warn = def < SUPPLY_WARNING_LEVEL && !f.def_supply_warned;
                let def_ok = def > SUPPLY_RESTORED_LEVEL && f.def_supply_warned;
                if att_warn {
                    f.att_supply_warned = true;
                } else if att_ok {
                    f.att_supply_warned = false;
                }
                if def_warn {
                    f.def_supply_warned = true;
                } else if def_ok {
                    f.def_supply_warned = false;
                }
                (att, def, att_warn, att_ok, def_warn, def_ok)
            };
            let (an, dn) = (self.names[a as usize].clone(), self.names[d as usize].clone());
            if att_warn {
                notes.push(format!("{} SUPPLY LINE CUT - ARMY EXHAUSTING", an));
            } else if att_ok {
                notes.push(format!("{} SUPPLY RESTORED", an));
            }
            if def_warn {
                notes.push(format!("{} SUPPLY LINE CUT - ARMY EXHAUSTING", dn));
            } else if def_ok {
                notes.push(format!("{} SUPPLY RESTORED", dn));
            }
            // isolation bleeds the troops allocated to this front, never an arbitrary share of the whole country
            if days > 0.0 {
                let a_rate = SUPPLY_EXTRA_DAILY_ATTRITION * (1.0 - att).powf(SUPPLY_ATTRITION_CURVE) * days;
                if a_rate > 0.0 && self.fronts[k].committed > 0.0 {
                    let c = self.fronts[k].committed;
                    self.fronts[k].committed = c - c.min(c * a_rate);
                }
                let d_rate = SUPPLY_EXTRA_DAILY_ATTRITION * (1.0 - def).powf(SUPPLY_ATTRITION_CURVE) * days;
                let dp = self.manpower[d as usize];
                let alloc = self.fronts[k].def_alloc.max(0.0);
                if d_rate > 0.0 && dp > 0.0 {
                    let lost = dp.min(alloc * d_rate);
                    self.manpower[d as usize] = (dp - lost).max(0.0);
                }
            }
        }
        for m in notes {
            self.say(m);
        }
    }

    fn rebuild_adjacency(&mut self, world: &World) {
        let (n, w, h) = (self.n, self.w, self.h);
        let owner = &world.map.owner;
        for v in self.adjacent.iter_mut() {
            *v = false;
        }
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let o = owner[i] as usize;
                if o == 0 {
                    continue;
                }
                if x + 1 < w {
                    let r = owner[i + 1] as usize;
                    if r != 0 && r != o {
                        self.adjacent[o * n + r] = true;
                        self.adjacent[r * n + o] = true;
                    }
                }
                if y + 1 < h {
                    let d = owner[i + w] as usize;
                    if d != 0 && d != o {
                        self.adjacent[o * n + d] = true;
                        self.adjacent[d * n + o] = true;
                    }
                }
            }
        }
    }

    fn build_frontier(&self, att: u16, def: u16, world: &World) -> Vec<usize> {
        let (w, h) = (self.w as i32, self.h as i32);
        let b = self.bbox[def as usize];
        let owner = &world.map.owner;
        let wu = self.w;
        let mut out = Vec::new();
        for y in b[1].max(0)..=b[3].min(h - 1) {
            for x in b[0].max(0)..=b[2].min(w - 1) {
                let i = (y * w + x) as usize;
                if owner[i] != def {
                    continue;
                }
                let touches = (x + 1 < w && owner[i + 1] == att)
                    || (x > 0 && owner[i - 1] == att)
                    || (y + 1 < h && owner[i + wu] == att)
                    || (y > 0 && owner[i - wu] == att);
                if touches {
                    out.push(i);
                }
            }
        }
        out
    }

    fn advance(&mut self, k: usize, dt: f32, world: &mut World) {
        let mut f = std::mem::take(&mut self.fronts[k]);
        self.advance_front(&mut f, dt, world);
        self.fronts[k] = f;
    }

    /// (resistance, casualty_share, recovery) -- _regroupLoad
    fn regroup_load(&self, f: &Front) -> (f32, f32, f32) {
        let ratio = self.power_ratio(f).max(0.001);
        let resistance = 1.0 / (1.0 + ratio / RATIO_PIVOT.max(0.001));
        let losses = f.push_combat_losses.max(0.0);
        let casualty = losses / (f.committed.max(0.0) + losses).max(1.0);
        let supply = f.att_supply.clamp(SUPPLY_MIN_READINESS, 1.0);
        let recovery = REGROUP_RECOVERY_PER_SEC
            * (REGROUP_RECOVERY_FLOOR + (1.0 - REGROUP_RECOVERY_FLOOR) * supply)
            * (1.0 + REGROUP_DOMINANCE_RECOVERY_BONUS * (1.0 - resistance));
        (resistance, casualty, recovery)
    }

    /// _tickRealism: returns true while the front is stalled (regrouping).
    fn tick_realism(&self, f: &mut Front, dt: f32) -> bool {
        if f.collapse {
            f.dig_in = 0.0;
            f.regrouping = false;
            f.regroup_work = 0.0;
            f.push_strain = 0.0;
            f.push_combat_losses = 0.0;
            return false;
        }
        if f.regrouping {
            let (_, _, recovery) = self.regroup_load(f);
            f.regroup_work = (f.regroup_work - dt.max(0.0) * recovery).max(0.0);
            if f.regroup_work <= 1e-8 {
                f.regrouping = false;
                f.push_strain = 0.0;
                f.push_combat_losses = 0.0;
            }
        }
        let stalled = f.regrouping;
        if !stalled {
            f.dig_in = (f.dig_in + dt / DIG_IN_SECONDS.max(0.001)).min(1.0);
        }
        stalled
    }

    /// _noteFrontAdvance: pushing hard builds strain; enough strain forces a regroup pause.
    fn note_front_advance(&self, f: &mut Front, gained: usize, orig_cells: f32) {
        if f.collapse || gained == 0 {
            return;
        }
        let (resistance, casualty, _) = self.regroup_load(f);
        let baseline = STALL_MIN_CELLS.max(orig_cells * STALL_CELLS_TRIGGER_FRAC);
        f.push_strain += gained as f32 / baseline.max(1.0)
            * (REGROUP_ADVANCE_STRAIN + REGROUP_RESISTANCE_STRAIN * resistance + REGROUP_LOSS_STRAIN * casualty);
        if f.push_strain >= 1.0 && !f.regrouping {
            f.regrouping = true;
            f.regroup_work = (REGROUP_BASE_WORK + REGROUP_RESISTANCE_WORK * resistance) * (1.0 + REGROUP_LOSS_STRAIN * casualty);
            f.cell_budget = 0.0;
        }
    }

    /// _findSmallestEnclave: smallest connected blob of the defender (<= max_size cells)
    /// that is surrounded only by the attacker (or water).
    fn find_enclave(&mut self, att: u16, def: u16, max_size: usize, world: &World) -> Option<Vec<usize>> {
        let (w, h) = (self.w as i32, self.h as i32);
        let wu = self.w;
        let b = self.bbox[def as usize];
        let owner = &world.map.owner;
        let land = &world.map.land;
        let limit = max_size.max(ENCLAVE_MAX_CELLS);
        self.enc_epoch = self.enc_epoch.wrapping_add(1);
        if self.enc_epoch == 0 {
            for v in self.enc_seen.iter_mut() {
                *v = 0;
            }
            self.enc_epoch = 1;
        }
        let epoch = self.enc_epoch;
        let mut best: Option<Vec<usize>> = None;
        let mut stack: Vec<usize> = Vec::new();
        for y in b[1].max(0)..=b[3].min(h - 1) {
            for x in b[0].max(0)..=b[2].min(w - 1) {
                let start = (y * w + x) as usize;
                if owner[start] != def || self.enc_seen[start] == epoch {
                    continue;
                }
                stack.clear();
                stack.push(start);
                self.enc_seen[start] = epoch;
                let mut head = 0usize;
                let mut count = 0usize;
                let mut cells: Vec<usize> = Vec::new();
                let mut enclosed = true;
                while head < stack.len() {
                    let i = stack[head];
                    head += 1;
                    count += 1;
                    if count <= limit {
                        cells.push(i);
                    }
                    let (cx, cy) = ((i % wu) as i32, (i / wu) as i32);
                    let mut nb = [usize::MAX; 4];
                    if cx > 0 { nb[0] = i - 1; }
                    if cx + 1 < w { nb[1] = i + 1; }
                    if cy > 0 { nb[2] = i - wu; }
                    if cy + 1 < h { nb[3] = i + wu; }
                    for &n in &nb {
                        if n == usize::MAX {
                            continue;
                        }
                        if owner[n] == def {
                            if self.enc_seen[n] != epoch {
                                self.enc_seen[n] = epoch;
                                stack.push(n);
                            }
                        } else if land[n] == 1 && owner[n] != att {
                            enclosed = false;
                        }
                    }
                }
                if count <= limit && count <= max_size && enclosed {
                    let better = match &best {
                        Some(bc) => cells.len() < bc.len(),
                        None => true,
                    };
                    if better {
                        best = Some(cells);
                    }
                }
            }
        }
        best
    }

    /// Hand `captured` cells (all owned by the defender) to the attacker and bookkeep.
    fn apply_capture(&mut self, f: &mut Front, captured: &[usize], world: &mut World) {
        let (a, d) = (f.attacker as usize, f.defender as usize);
        let (w, h) = (self.w as i32, self.h as i32);
        let defender = f.defender;
        let mut bx = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        let mut new_front: Vec<usize> = Vec::new();
        let tick = self.tick;
        for &i in captured {
            world.set_owner(i, f.attacker);
            self.recent.insert(i, tick);
            let (x, y) = ((i % self.w) as i32, (i / self.w) as i32);
            bx = [bx[0].min(x), bx[1].min(y), bx[2].max(x), bx[3].max(y)];
            if x + 1 < w { new_front.push(i + 1); }
            if x > 0 { new_front.push(i - 1); }
            if y + 1 < h { new_front.push(i + self.w); }
            if y > 0 { new_front.push(i - self.w); }
        }
        let n_cap = captured.len() as u32;
        self.cells[d] = self.cells[d].saturating_sub(n_cap);
        self.cells[a] += n_cap;
        {
            let ab = &mut self.bbox[a];
            ab[0] = ab[0].min(bx[0]);
            ab[1] = ab[1].min(bx[1]);
            ab[2] = ab[2].max(bx[2]);
            ab[3] = ab[3].max(bx[3]);
        }
        new_front.retain(|&i| world.map.owner[i] == defender);
        f.frontier.retain(|&i| world.map.owner[i] == defender);
        f.frontier.extend(new_front);
        f.frontier.sort_unstable();
        f.frontier.dedup();
        self.dirty.push(bx);
        if self.cells[d] == 0 {
            self.eliminate(f.defender);
            f.done = true;
        }
    }

    fn capture_pocket(&mut self, f: &mut Front, cells: &[usize], cost: f32, world: &mut World) {
        if cells.is_empty() {
            f.done = true;
            return;
        }
        f.committed = (f.committed - cells.len() as f32 * cost).max(0.0);
        self.credit_occupation(f, cells.len());
        self.apply_capture(f, cells, world);
    }

    /// Mop-up: grab a small pocket the attacker has fully surrounded. Returns true if it captured.
    fn try_pocket(&mut self, f: &mut Front, cap: usize, cost: f32, take: usize, world: &mut World) -> bool {
        if f.enc_tick == 0 || self.tick - f.enc_tick >= ENC_RECHECK_TICKS {
            f.enc_tick = self.tick;
            f.enc_pocket = self.find_enclave(f.attacker, f.defender, cap, world);
        }
        let mut pocket = f.enc_pocket.take();
        if let Some(p) = &pocket {
            if p.iter().any(|&i| world.map.owner[i] != f.defender) {
                pocket = None;
            }
        }
        let Some(p) = pocket else { return false };
        let tick = self.tick;
        let held: Vec<usize> = p
            .iter()
            .copied()
            .filter(|i| self.recent.get(i).map_or(true, |&rt| tick - rt >= RECAPTURE_HOLD_TICKS))
            .take(take)
            .collect();
        if held.is_empty() {
            f.enc_pocket = Some(p);
            return false;
        }
        self.capture_pocket(f, &held, cost, world);
        true
    }

    fn advance_front(&mut self, f: &mut Front, dt: f32, world: &mut World) {
        let (a, d) = (f.attacker as usize, f.defender as usize);
        if self.eliminated[a] || self.eliminated[d] || !self.at_war[a * self.n + d] {
            f.done = true;
            return;
        }
        if f.collapse {
            f.collapse_t += dt;
        }
        // --- _applyFrontAttrition
        let ratio = self.power_ratio(f);
        let att_edge = clamp01(ratio - 1.0);
        let def_edge = clamp01(1.0 - ratio);
        let def_rate = MAX_DRAIN.min(DEF_BASE * (1.0 + att_edge * 2.0));
        let att_rate = MAX_DRAIN.min(ATT_BASE * (1.0 + def_edge * 1.5) * FIELD_DRAIN);
        let exposed_def = self.manpower[d].min(f.def_alloc.max(0.0)).min(f.committed.max(0.0));
        if exposed_def > 0.0 {
            self.manpower[d] = (self.manpower[d] - exposed_def * (1.0 - (-def_rate * dt).exp())).max(0.0);
        }
        if f.committed > 0.0 {
            let exposed_att = f.committed.min(f.def_alloc.max(0.0));
            let lost = exposed_att * (1.0 - (-att_rate * dt).exp());
            f.committed = (f.committed - lost).max(0.0);
            f.push_combat_losses += lost;
            self.add_vp_progress(f.defender, 1, lost, VP_KILLS_PER_POINT);
        }
        let naval_active = self.naval_tick(f, dt, world);
        if f.done {
            return;
        }
        // --- _tickRealism
        let stalled = self.tick_realism(f, dt);
        let def_orig = self.orig_cells[d].max(1) as f32;
        let enclave_cap = ((def_orig * ENCLAVE_MAX_FRAC).floor() as usize).clamp(1, ENCLAVE_MAX_CELLS);
        if stalled {
            f.cell_budget = 0.0;
            if self.cells[d] > 0 {
                let cost = self.cost_per_cell(f);
                self.try_pocket(f, enclave_cap, cost, usize::MAX, world);
            }
            return;
        }
        // --- _advance
        let ratio = self.power_ratio(f);
        if ratio < ADVANCE_MIN_RATIO && !f.collapse {
            f.cell_budget = 0.0;
            return;
        }
        let cost = self.cost_per_cell(f);
        let cap = (f.committed / cost).floor();
        if cap <= 0.0 {
            f.done = true;
            return;
        }
        let speed = self.speed_mult(f);
        f.cell_budget += FRONT_CELLS_PER_SEC * dt * speed;
        let take = f.cell_budget.floor().min(cap) as usize;
        f.cell_budget -= take as f32;

        if take > 0 && f.committed < cost * MOPUP_MULT {
            if self.try_pocket(f, enclave_cap, cost, take, world) {
                return;
            }
        }

        let defender = f.defender;
        f.frontier.retain(|&i| world.map.owner[i] == defender);
        if f.frontier.is_empty() || self.tick >= f.rebuild_at {
            f.frontier = self.build_frontier(f.attacker, f.defender, world);
            f.rebuild_at = self.tick + 16;
        }
        if f.frontier.is_empty() {
            if !naval_active {
                f.done = true; // the two countries no longer touch and nothing is sailing
            }
            return;
        }
        if take == 0 {
            return;
        }
        // cells captured in the last few ticks can't be flipped straight back
        let tick = self.tick;
        let elig: Vec<usize> = f
            .frontier
            .iter()
            .copied()
            .filter(|i| self.recent.get(i).map_or(true, |&rt| tick - rt >= RECAPTURE_HOLD_TICKS))
            .collect();
        if elig.is_empty() {
            return;
        }
        let captured: Vec<usize> = if elig.len() <= MIN_REMNANT_CELLS || take >= elig.len() {
            elig
        } else {
            let seeds = if speed > 2.2 { 2 } else { 1 };
            self.pick_cells(&elig, take, seeds)
        };
        if captured.is_empty() {
            return;
        }
        f.committed = (f.committed - captured.len() as f32 * cost).max(0.0);
        if !f.collapse {
            let gained = captured.len();
            f.dig_in = (f.dig_in - gained as f32 / def_orig * 6.0).max(0.0);
            self.note_front_advance(f, gained, def_orig);
        }
        self.credit_occupation(f, captured.len());
        self.apply_capture(f, &captured, world);
    }

    /// Bite a blob (or two) out of the frontier: the `take` cells closest to random seeds.
    fn pick_cells(&mut self, frontier: &[usize], take: usize, seeds: usize) -> Vec<usize> {
        let w = self.w;
        let per = (take / seeds).max(1);
        let mut chosen: Vec<usize> = Vec::with_capacity(take);
        for _ in 0..seeds {
            let s = frontier[self.rng.range(frontier.len())];
            let (sx, sy) = ((s % w) as i32, (s / w) as i32);
            let mut v: Vec<(i32, usize)> = frontier
                .iter()
                .map(|&c| {
                    let dx = (c % w) as i32 - sx;
                    let dy = (c / w) as i32 - sy;
                    (dx * dx + dy * dy, c)
                })
                .collect();
            let m = per.min(v.len());
            if m == 0 {
                continue;
            }
            if m < v.len() {
                v.select_nth_unstable_by_key(m - 1, |e| e.0);
                v.truncate(m);
            }
            chosen.extend(v.into_iter().map(|e| e.1));
        }
        chosen.sort_unstable();
        chosen.dedup();
        chosen
    }

    fn eliminate(&mut self, id: u16) {
        let i = id as usize;
        self.eliminated[i] = true;
        self.manpower[i] = 0.0;
        for b in 0..self.n {
            self.at_war[i * self.n + b] = false;
            self.at_war[b * self.n + i] = false;
        }
        for f in self.fronts.iter_mut() {
            if f.attacker == id || f.defender == id {
                f.done = true;
            }
        }
        self.peace_offers.retain(|&x| x != id);
        let msg = format!("{} HAS BEEN ELIMINATED", self.names[i]);
        self.say(msg);
    }

    fn check_capitals(&mut self, world: &World) {
        for id in 1..self.n {
            let Some(cap) = self.capital[id] else { continue };
            if self.eliminated[id] {
                continue;
            }
            let holder = world.map.owner[cap] as usize;
            if holder == id {
                if self.capital_fallen[id] {
                    self.capital_fallen[id] = false; // recaptured
                    for f in self.fronts.iter_mut() {
                        if f.defender as usize == id {
                            f.collapse = false;
                        }
                    }
                }
            } else if !self.capital_fallen[id] && holder != 0 && self.at_war[id * self.n + holder] {
                self.capital_fallen[id] = true;
                for f in self.fronts.iter_mut() {
                    if f.defender as usize == id {
                        f.collapse = true;
                        f.collapse_t = 0.0;
                    }
                }
                let msg = format!("THE CAPITAL OF {} HAS FALLEN", self.names[id]);
                self.say(msg);
            }
        }
    }

    /// _regenManpower (population health / events / ideology multipliers omitted).
    fn regen(&mut self, dt_days: f32) {
        for id in 1..self.n {
            if self.eliminated[id] {
                continue;
            }
            let cur = self.cells[id] as f32;
            if cur <= 0.0 {
                self.manpower[id] = 0.0;
                continue;
            }
            let orig = self.orig_cells[id].max(1) as f32;
            let tf = clamp01(cur / orig);
            let territory_factor = TERRITORY_FLOOR + (1.0 - TERRITORY_FLOOR) * tf;
            let fallen = self.capital_fallen[id];
            let capital_factor = if fallen { CAPITAL_FALLEN_MULT } else if self.game_days < self.crisis_until[id] { 0.5 } else { 1.0 };
            let loss = clamp01(1.0 - tf);
            let ramp = clamp01(loss / SURGE_LOSS_REF);
            let surge = SURGE_MAX.min(1.0 + ramp * (SURGE_MAX - 1.0) + if fallen { SURGE_CAPITAL_BONUS } else { 0.0 });
            let scale = self.territory_scale(id);
            let ceiling = self.max_pool[id] * scale;
            let committed = self.committed_total(id as u16);
            let throttle = MIN_OFFENSIVE_THROTTLE.max(1.0 - clamp01(committed / ceiling.max(1.0)));
            let r_eff = self.regen_r[id] * territory_factor * capital_factor * surge * throttle * self.mods[id][ideology::K_MP_RATE] * self.mods[id][ideology::K_MP_CAP];
            let pool = self.manpower[id].min(ceiling);
            let rdt = r_eff * dt_days;
            let base = pool.max(ceiling.min(self.min_seed[id] * scale));
            self.manpower[id] = ceiling.min((pool + rdt * base) / (1.0 + rdt * base / ceiling));
        }
    }

    // ------------------------------------------------------------------ AI
    fn ai_pass(&mut self, world: &World) {
        let n = self.n;
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for a in 1..n {
            for b in (a + 1)..n {
                if self.at_war[a * n + b] {
                    pairs.push((a, b));
                }
            }
        }
        for (a, b) in pairs {
            self.ai_commit(a as u16, b as u16, false);
            self.ai_commit(b as u16, a as u16, false);
        }
        self.ally_pass();
        self.assist_pass();
        self.ai_declarations();
        self.ai_missiles(world);
    }

    fn ai_commit(&mut self, a: u16, b: u16, retaliate: bool) {
        if self.is_human(a) || self.eliminated[a as usize] {
            return;
        }
        let n = self.n;
        let key = a as usize * n + b as usize;
        if !retaliate && self.commit_cd[key] > self.time {
            return;
        }
        let pool = self.manpower[a as usize];
        if pool <= 0.0 {
            return;
        }
        let existing = self.fronts.iter().find(|f| f.attacker == a && f.defender == b && !f.done).map(|f| f.committed);
        let want = match existing {
            None => true,
            Some(c) => c < MIN_COMMIT * 0.5 && pool >= MIN_COMMIT,
        };
        if !want {
            return;
        }
        let mut frac = COMMIT_FRACTION;
        if retaliate {
            frac = (frac * AI_RETALIATE_BONUS).min(AI_RETALIATE_CAP);
        }
        let amount = (pool * frac).min(pool * (1.0 - AI_ARMY_RESERVE));
        self.commit_cd[key] = self.time + AI_COMMIT_COOLDOWN_S;
        self.commit(a, b, amount);
    }

    fn ai_declarations(&mut self) {
        let n = self.n;
        if n <= 2 {
            return;
        }
        let count = n - 1;
        let start = self.ai_cursor;
        let mut visited = 0;
        let mut attempts = 1;
        while visited < count && attempts > 0 {
            let id = 1 + (start + visited) % count;
            visited += 1;
            if self.eliminated[id] || self.is_human(id as u16) {
                continue;
            }
            let aggr = self.aggression[id];
            if aggr < AI_MIN_AGGRESSION {
                continue;
            }
            let att_mp = self.manpower[id];
            if att_mp <= 0.0 {
                continue;
            }
            let mut best: Option<usize> = None;
            let mut best_score = -1.0f32;
            for t in 1..n {
                if t == id || self.eliminated[t] || self.is_human(t as u16) {
                    continue; // medium difficulty: the AI never opens a war against the player
                }
                let key = id * n + t;
                if self.at_war[key] || self.allies[key] || self.peace_lock[key] > self.time || self.consider_cd[key] > self.time || !self.adjacent[key] {
                    continue;
                }
                let score = clamp01(att_mp / (att_mp + self.manpower[t].max(1.0)));
                if score > best_score {
                    best_score = score;
                    best = Some(t);
                }
            }
            let Some(t) = best else { continue };
            self.consider_cd[id * n + t] = self.time + AI_DECLARE_INTERVAL_S;
            let resolve = (AI_DECLARE_SCORE + (1.0 - aggr) * 0.08).clamp(0.44, 0.82);
            if best_score <= resolve {
                continue;
            }
            if self.declare_war(id as u16, t as u16) {
                attempts -= 1;
            }
        }
        self.ai_cursor = (start + visited.max(1)) % count;
    }

    fn peace_pass(&mut self, elapsed: f32, world: &World) {
        let n = self.n;
        let mut peaces: Vec<(u16, u16)> = Vec::new();
        let mut offers: Vec<u16> = Vec::new();
        // AI attackers cash in: they hold a worthwhile slice of an enemy and can already afford to annex it
        for k in 0..self.fronts.len() {
            if self.fronts[k].done {
                continue;
            }
            let (a, d) = (self.fronts[k].attacker, self.fronts[k].defender);
            if self.is_human(a) || self.eliminated[a as usize] || self.eliminated[d as usize] {
                continue;
            }
            let total = self.orig_cells[d as usize].max(1) as f32;
            let held = world.held(a, d);
            if held == 0 || (held as f32) / total < AI_BANK_PEACE_FRAC {
                continue;
            }
            if self.offer_cd[a as usize] > self.time || self.peace_offers.contains(&a) {
                continue;
            }
            if self.crowns[a as usize] < self.annex_cost_cells(d as usize, held) {
                continue;
            }
            if self.is_human(d) {
                offers.push(a);
            } else if (held as f32) / total >= AI_FORCED_PEACE_LOSS || self.ai_wants_peace(d, a) {
                peaces.push((a, d));
            } else {
                self.offer_cd[a as usize] = self.time + OFFER_COOLDOWN_S;
            }
        }
        for f in self.fronts.iter().filter(|f| !f.done) {
            let (a, d) = (f.attacker, f.defender);
            if self.is_human(d) || !self.ai_wants_peace(d, a) {
                continue;
            }
            if self.is_human(a) {
                offers.push(d);
            } else {
                peaces.push((a, d));
            }
        }
        for o in offers {
            if !self.peace_offers.contains(&o) && self.offer_cd[o as usize] <= self.time {
                self.peace_offers.push(o);
                let msg = format!("{} ASKS YOU FOR PEACE", self.names[o as usize]);
                self.say(msg);
            }
        }
        for (a, d) in peaces {
            if self.is_at_war(a, d) {
                self.make_peace(a, d);
                let msg = format!("PEACE: {} AND {}", self.names[a as usize], self.names[d as usize]);
                self.say(msg);
            }
        }
        // stale wars
        for a in 1..n {
            for b in (a + 1)..n {
                if !self.at_war[a * n + b] {
                    continue;
                }
                if self.no_front_between(a as u16, b as u16) {
                    self.war_idle[a * n + b] += elapsed;
                    if self.war_idle[a * n + b] >= STALE_WAR_S {
                        self.war_idle[a * n + b] = 0.0;
                        self.make_peace(a as u16, b as u16);
                        let msg = format!("WAR BETWEEN {} AND {} FIZZLES OUT", self.names[a], self.names[b]);
                        self.say(msg);
                    }
                } else {
                    self.war_idle[a * n + b] = 0.0;
                }
            }
        }
    }
}
