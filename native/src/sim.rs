//! Core war simulation: manpower, fronts, territory capture, capital fall, AI wars + peace.
//!
//! Ported from the JS `WarSim` (formulas and constants are the originals):
//!   frontPowerRatio / frontSpeedMult / effCostPerCell, _applyFrontAttrition,
//!   _updateDefenseAllocations / _defenseComponents, _regenManpower, _buildRegenCache,
//!   _runAiDeclarations, _aiWantsPeace, capital-fall collapse.
//! NOT ported yet (approximated or skipped): supply, dig-in/regroup realism, focused attacks,
//! naval invasions, alliances/assists, buildings/economy, missiles/nukes, ideology/policy,
//! empires, victory points, population sim, news feed, enclave capture.
use crate::econ::{Econ, EconCtx};
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
    enc_tick: u64,
    enc_pocket: Option<Vec<usize>>,
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
        let econ = Econ::new(m, seed ^ 0x9E37_79B9);
        let pop: Vec<f32> = m.countries.iter().map(|c| c.pop).collect();
        let gdp: Vec<f32> = m.countries.iter().map(|c| c.gdp).collect();
        let pop_base: Vec<f32> = pop.iter().map(|p| p.max(0.05)).collect();
        let live_gdp = gdp.clone();
        let prev_cells = cells.clone();
        let density_ref = pop_base.iter().skip(1).sum::<f32>() / (cells.iter().sum::<u32>() as f32).max(1.0);
        let orig_cells = cells.clone();
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
        let def_cmd = 1.0 - COMMAND_DEFENSE_WEAKNESS * if self.capital_fallen[d] { 1.0 } else { 0.0 };
        let att = (f.committed * att_cmd).max(1.0);
        let dig = 1.0 + f.dig_in.clamp(0.0, 1.0) * DIG_IN_MAX_BONUS;
        let def = (f.def_alloc * def_cmd * dig).max(1.0);
        (att / def).powf(RATIO_CURVE)
    }

    fn speed_mult(&self, f: &Front) -> f32 {
        let ratio_mult = self.power_ratio(f).clamp(SPEED_MIN, SPEED_MAX);
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

    pub fn make_peace(&mut self, a: u16, b: u16) {
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
            let r_eff = pop_r * if at_war { POP_WAR_GROWTH_MULT } else { 1.0 };
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

        self.adj_timer += sd;
        if self.adj_timer >= 1.0 {
            self.adj_timer = 0.0;
            self.rebuild_adjacency(world);
        }
        self.update_defense_allocs(sd);
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
        self.fronts.retain(|f| !f.done);
        if self.tick & 63 == 0 && self.recent.len() > 4000 {
            let cut = self.tick.saturating_sub(RECAPTURE_HOLD_TICKS * 4);
            self.recent.retain(|_, v| *v >= cut);
        }

        self.ai_timer += sd;
        if self.ai_timer >= AI_PASS_S {
            self.ai_timer = 0.0;
            self.ai_pass();
        }
        self.peace_timer += sd;
        if self.peace_timer >= 2.0 {
            let t = self.peace_timer;
            self.peace_timer = 0.0;
            self.peace_pass(t);
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
            };
            self.econ.step(sd, &ctx, &world.map, self.player, self.time);
        }
        let now = self.time;
        self.log.retain(|(_, t)| now - *t < 30.0);
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

    /// (resistance, casualty_share, recovery) -- _regroupLoad (supply is not ported: full supply assumed)
    fn regroup_load(&self, f: &Front) -> (f32, f32, f32) {
        let ratio = self.power_ratio(f).max(0.001);
        let resistance = 1.0 / (1.0 + ratio / RATIO_PIVOT.max(0.001));
        let losses = f.push_combat_losses.max(0.0);
        let casualty = losses / (f.committed.max(0.0) + losses).max(1.0);
        let supply = 1.0f32;
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
            f.done = true; // the two countries no longer touch
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
            let capital_factor = if fallen { CAPITAL_FALLEN_MULT } else { 1.0 };
            let loss = clamp01(1.0 - tf);
            let ramp = clamp01(loss / SURGE_LOSS_REF);
            let surge = SURGE_MAX.min(1.0 + ramp * (SURGE_MAX - 1.0) + if fallen { SURGE_CAPITAL_BONUS } else { 0.0 });
            let scale = self.territory_scale(id);
            let ceiling = self.max_pool[id] * scale;
            let committed = self.committed_total(id as u16);
            let throttle = MIN_OFFENSIVE_THROTTLE.max(1.0 - clamp01(committed / ceiling.max(1.0)));
            let r_eff = self.regen_r[id] * territory_factor * capital_factor * surge * throttle;
            let pool = self.manpower[id].min(ceiling);
            let rdt = r_eff * dt_days;
            let base = pool.max(ceiling.min(self.min_seed[id] * scale));
            self.manpower[id] = ceiling.min((pool + rdt * base) / (1.0 + rdt * base / ceiling));
        }
    }

    // ------------------------------------------------------------------ AI
    fn ai_pass(&mut self) {
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

    fn peace_pass(&mut self, elapsed: f32) {
        let n = self.n;
        let mut peaces: Vec<(u16, u16)> = Vec::new();
        let mut offers: Vec<u16> = Vec::new();
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
