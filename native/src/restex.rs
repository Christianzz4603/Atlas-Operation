//! Generated: resource-bar icons (downscaled from the HTML game's res-icon images).

pub fn res_bytes(name: &str) -> Option<&'static [u8]> {
    match name {
        "population" => Some(include_bytes!("../assets/res/population.png")),
        "bricks" => Some(include_bytes!("../assets/res/bricks.png")),
        "food" => Some(include_bytes!("../assets/res/food.png")),
        "money" => Some(include_bytes!("../assets/res/money.png")),
        "vp" => Some(include_bytes!("../assets/res/vp.png")),
        "steel" => Some(include_bytes!("../assets/res/steel.png")),
        "oil" => Some(include_bytes!("../assets/res/oil.png")),
        "uranium" => Some(include_bytes!("../assets/res/uranium.png")),
        _ => None,
    }
}
