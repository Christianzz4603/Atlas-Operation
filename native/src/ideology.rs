//! Regimes (ideologies) and national policy sliders -- port of CONFIG.ideology / CONFIG.policy and
//! WarSim.ideologyMods / policyMultipliers / combineWithPolicy.
//!
//! Every multiplier flows through ONE hook (`mods_for`): base regime bundle * policy sliders. Consumers read the
//! cached bundle per nation from `Sim` (combat speed, population growth, manpower regen, money, production).
//! Empire perks (WarSim.applyEmpireMods) are not ported yet.

pub const K_MONEY: usize = 0;
pub const K_PROD: usize = 1;
pub const K_MP_RATE: usize = 2;
pub const K_MP_CAP: usize = 3;
pub const K_POP: usize = 4;
pub const K_ATT: usize = 5;
pub const K_DEF: usize = 6;
pub const NKEYS: usize = 7;

/// money, production, manpowerRate, manpowerCap, popGrowth, attack, defense
pub type Mods = [f32; NKEYS];
pub const NEUTRAL: Mods = [1.0; NKEYS];
pub const KEY_LABELS: [&str; NKEYS] = ["Money", "Production", "Recruits", "Reserve recruits", "Pop. growth", "Offense", "Defense"];

pub const COOLDOWN_DAYS: f32 = 120.0;
pub const SWITCH_MONEY_GDP_FRAC: f32 = 0.02;
pub const UNREST_WAR_MULT: f32 = 1.75;

pub const POLICY_NEUTRAL: f32 = 50.0;
pub const AI_MIN: f32 = 20.0;
pub const AI_MAX: f32 = 80.0;

// ===== EMPIRE SYSTEM (CONFIG.empire) =====
// A nation becomes an Empire only after real conquest (foreign land held) plus a crowns bankroll plus a
// manpower floor. Perk mods stack onto the regime+policy bundle through the SAME `mods_for` pipeline (see
// apply_empire_perks), so there is still exactly one door into the multipliers.
pub const EMPIRE_REQUIRE_FOREIGN_CELLS: u32 = 60;
pub const EMPIRE_REQUIRE_VP: u32 = 70;
pub const EMPIRE_REQUIRE_MANPOWER: f32 = 40.0;
pub const EMPIRE_FORM_VP_COST: u32 = 35;
pub const EMPIRE_MAX_ACTIVE_PERKS: usize = 5;
pub const EMPIRE_SHOP_PRICE_MULT: f32 = 3.5;
pub const EMPIRE_AI_CHECK_S: f32 = 5.0;
pub const EMPIRE_AI_FORM_CHANCE: f32 = 0.35;
pub const REPUBLIC_SAVE_BASE_COST: f32 = 3.0;
pub const REPUBLIC_SAVE_COST_PER_300_CELLS: f32 = 1.0;

pub struct Perk {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub stat_key: usize,
    pub mult: f32,
    pub cost: u32,
}

pub const PERKS: [Perk; 14] = [
    Perk { id: "mercantile", name: "Mercantile Charter", blurb: "Imperial trade routes swell the treasury.", stat_key: K_MONEY, mult: 1.15, cost: 20 },
    Perk { id: "tribute", name: "Tribute System", blurb: "Subject provinces pay tribute to the crown.", stat_key: K_MONEY, mult: 1.10, cost: 14 },
    Perk { id: "industrious", name: "Industrious Colonies", blurb: "Overseas holdings feed the factories.", stat_key: K_PROD, mult: 1.15, cost: 20 },
    Perk { id: "workshops", name: "Imperial Workshops", blurb: "State workshops standardize output.", stat_key: K_PROD, mult: 1.10, cost: 14 },
    Perk { id: "conscription", name: "Imperial Conscription", blurb: "Subject peoples swell the muster rolls.", stat_key: K_MP_RATE, mult: 1.15, cost: 22 },
    Perk { id: "recruiters", name: "Colonial Recruiters", blurb: "Recruiting offices open across the provinces.", stat_key: K_MP_RATE, mult: 1.10, cost: 16 },
    Perk { id: "levy", name: "Grand Levy", blurb: "Reserve recruitment moves faster.", stat_key: K_MP_CAP, mult: 1.18, cost: 24 },
    Perk { id: "reserves", name: "Standing Reserves", blurb: "Reserve offices recruit faster.", stat_key: K_MP_CAP, mult: 1.10, cost: 15 },
    Perk { id: "settlement", name: "Settlement Drive", blurb: "Resettlement swells the population.", stat_key: K_POP, mult: 1.12, cost: 16 },
    Perk { id: "granaries", name: "Imperial Granaries", blurb: "Full granaries let families grow.", stat_key: K_POP, mult: 1.08, cost: 11 },
    Perk { id: "legions", name: "Legion Doctrine", blurb: "Veteran legions press the offensive.", stat_key: K_ATT, mult: 1.12, cost: 26 },
    Perk { id: "vanguard", name: "Imperial Vanguard", blurb: "Elite vanguard units spearhead every assault.", stat_key: K_ATT, mult: 1.08, cost: 17 },
    Perk { id: "fortress", name: "Frontier Fortresses", blurb: "Border fortresses blunt every invasion.", stat_key: K_DEF, mult: 1.14, cost: 22 },
    Perk { id: "garrisons", name: "Provincial Garrisons", blurb: "Every province keeps its own garrison.", stat_key: K_DEF, mult: 1.09, cost: 15 },
];

/// WarSim.applyEmpireMods: perks are plain multipliers on the same stat keys regime/policy already use, so
/// stacking is always base*regime*policy*perk1*perk2 -- one order, no double-counting.
pub fn apply_empire_perks(mut base: Mods, perks: &[u8]) -> Mods {
    for &pid in perks {
        if let Some(p) = PERKS.get(pid as usize) {
            base[p.stat_key] *= p.mult;
        }
    }
    base
}

pub struct Regime {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub cost_mult: f32,
    pub mods: Mods,
    /// starting slider positions: socialSpending, militaryFunding, industrialFocus, taxation
    pub policy: [f32; 4],
}

pub const REGIMES: [Regime; 12] = [
    Regime { id: "liberal_democracy", name: "Liberal Democracy", blurb: "Open markets, weak conscription.", cost_mult: 1.0,
        mods: [1.10, 1.00, 0.90, 0.85, 1.10, 0.90, 1.10], policy: [55.0, 30.0, 45.0, 45.0] },
    Regime { id: "social_democracy", name: "Social Democracy", blurb: "Welfare state; slow to march, hard to break.", cost_mult: 1.0,
        mods: [1.00, 1.05, 0.95, 0.95, 1.15, 0.85, 1.15], policy: [80.0, 35.0, 50.0, 70.0] },
    Regime { id: "conservatism", name: "Conservatism", blurb: "Steady, unremarkable, cheap to install.", cost_mult: 0.8,
        mods: [1.05, 1.00, 1.00, 1.00, 1.00, 1.00, 1.05], policy: [45.0, 55.0, 50.0, 50.0] },
    Regime { id: "free_market", name: "Free Market Capitalism", blurb: "Money floods in, nobody wants to enlist.", cost_mult: 1.1,
        mods: [1.22, 1.10, 0.85, 0.78, 1.00, 0.90, 0.90], policy: [15.0, 25.0, 65.0, 20.0] },
    Regime { id: "communism", name: "Communism", blurb: "Planned industry, mass mobilization, poor treasury.", cost_mult: 1.6,
        mods: [0.85, 1.25, 1.20, 1.25, 0.95, 1.05, 1.10], policy: [75.0, 75.0, 90.0, 85.0] },
    Regime { id: "socialism", name: "Democratic Socialism", blurb: "Industry over finance, without the purges.", cost_mult: 1.2,
        mods: [0.90, 1.15, 1.10, 1.10, 1.05, 1.00, 1.05], policy: [70.0, 55.0, 75.0, 75.0] },
    Regime { id: "anarcho_syndicalism", name: "Anarcho-Syndicalism", blurb: "No state to tax. Militias defend fanatically.", cost_mult: 1.5,
        mods: [0.78, 1.12, 1.15, 1.05, 1.00, 0.82, 1.32], policy: [65.0, 45.0, 60.0, 10.0] },
    Regime { id: "fascism", name: "Fascism", blurb: "Total war economy. Offensives hit hardest.", cost_mult: 1.8,
        mods: [0.95, 1.10, 1.25, 1.20, 0.88, 1.28, 1.00], policy: [35.0, 95.0, 85.0, 65.0] },
    Regime { id: "military_junta", name: "Military Junta", blurb: "Army first, economy last.", cost_mult: 1.4,
        mods: [0.85, 0.95, 1.30, 1.15, 0.90, 1.15, 1.15], policy: [20.0, 90.0, 45.0, 60.0] },
    Regime { id: "absolute_monarchy", name: "Absolute Monarchy", blurb: "Crown revenue, sluggish industry.", cost_mult: 1.3,
        mods: [1.08, 0.92, 1.05, 1.00, 0.95, 1.05, 1.05], policy: [30.0, 60.0, 35.0, 70.0] },
    Regime { id: "theocracy", name: "Theocracy", blurb: "Faithful recruits, defensive zeal, stagnant industry.", cost_mult: 1.4,
        mods: [0.95, 0.92, 1.15, 1.10, 1.12, 0.98, 1.22], policy: [60.0, 50.0, 30.0, 55.0] },
    Regime { id: "technocracy", name: "Technocracy", blurb: "Optimized output, unenthusiastic conscription.", cost_mult: 1.5,
        mods: [1.15, 1.20, 0.88, 0.88, 1.05, 0.95, 1.00], policy: [50.0, 35.0, 85.0, 60.0] },
];

pub struct Slider {
    pub id: &'static str,
    pub name: &'static str,
    pub low: &'static str,
    pub high: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    /// per-key coefficient: mult = max(0.2, 1 + t * coef), t = (value - NEUTRAL) / (max - min)
    pub coef: Mods,
}

pub const SLIDERS: [Slider; 4] = [
    Slider { id: "socialSpending", name: "Social Spending", low: "Austerity", high: "Welfare", min: 0.0, max: 100.0, step: 5.0,
        coef: [-0.20, 0.0, -0.10, 0.0, 0.24, 0.0, 0.08] },
    Slider { id: "militaryFunding", name: "Military Funding", low: "Disarmed", high: "Total war", min: 0.0, max: 100.0, step: 5.0,
        coef: [-0.22, -0.06, 0.26, 0.0, 0.0, 0.18, 0.12] },
    Slider { id: "industrialFocus", name: "Industrial Focus", low: "Agrarian", high: "Heavy industry", min: 0.0, max: 100.0, step: 5.0,
        coef: [0.06, 0.28, 0.0, -0.08, -0.14, 0.0, 0.0] },
    Slider { id: "taxation", name: "Taxation", low: "Low tax", high: "High tax", min: 0.0, max: 100.0, step: 5.0,
        coef: [0.30, -0.14, -0.06, 0.0, -0.08, 0.0, 0.0] },
];

/// snapPolicy: round to the slider's step and clamp to its range.
pub fn snap(sl: &Slider, v: f32) -> f32 {
    let step = if sl.step > 0.0 { sl.step } else { 1.0 };
    ((v / step).round() * step).clamp(sl.min, sl.max)
}

pub fn default_policy() -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for (i, sl) in SLIDERS.iter().enumerate() {
        out[i] = snap(sl, POLICY_NEUTRAL);
    }
    out
}

/// ideologyPolicy: the slider platform a regime installs when it takes power.
pub fn regime_policy(regime: usize) -> [f32; 4] {
    let mut out = default_policy();
    if let Some(r) = REGIMES.get(regime) {
        for (i, sl) in SLIDERS.iter().enumerate() {
            out[i] = snap(sl, r.policy[i]);
        }
    }
    out
}

/// policyMultipliers
pub fn policy_multipliers(values: &[f32; 4]) -> Mods {
    let mut out = NEUTRAL;
    for (si, sl) in SLIDERS.iter().enumerate() {
        let raw = values[si];
        if !raw.is_finite() {
            continue;
        }
        let span = (sl.max - sl.min).max(1.0);
        let t = (raw.clamp(sl.min, sl.max) - POLICY_NEUTRAL) / span;
        for k in 0..NKEYS {
            if sl.coef[k] != 0.0 {
                out[k] *= (1.0 + t * sl.coef[k]).max(0.2);
            }
        }
    }
    out
}

/// ideologyMods = combineWithPolicy(regime bundle, sliders)
pub fn mods_for(regime: usize, policy: &[f32; 4]) -> Mods {
    let base = REGIMES.get(regime).map(|r| r.mods).unwrap_or(NEUTRAL);
    let pol = policy_multipliers(policy);
    let mut out = NEUTRAL;
    for k in 0..NKEYS {
        out[k] = base[k] * pol[k];
    }
    out
}

pub fn hash_str(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Deterministic 0..1 from (hash, game seed).
pub fn hash01(h: u32, seed: u32) -> f32 {
    let mut x = h ^ seed.wrapping_mul(0x9E37_79B1);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    (x >> 8) as f32 / 16777216.0
}

/// Starting regime roll (uniform over the list, seeded per game + country).
pub fn roll_regime(name: &str, seed: u32) -> usize {
    let r = hash01(hash_str(&format!("{}::ideology", name)), seed);
    ((r * REGIMES.len() as f32) as usize).min(REGIMES.len() - 1)
}

/// Starting slider roll: AI_MIN..AI_MAX per slider, snapped.
pub fn roll_policy(name: &str, seed: u32) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for (i, sl) in SLIDERS.iter().enumerate() {
        let lo = sl.min.max(AI_MIN);
        let hi = sl.max.min(AI_MAX);
        let r = hash01(hash_str(&format!("{}::policy::{}", name, sl.id)), seed);
        out[i] = snap(sl, lo + r * (hi - lo));
    }
    out
}
