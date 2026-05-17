#[derive(Debug)]
pub enum MarketCondition {
    InASlump,
    ChuggingAlong,
    LookingHot,
    HotHotHot,
}

impl MarketCondition {
    pub fn is_hot_hot_hot(&self) -> bool {
        matches!(self, Self::HotHotHot)
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InASlump => "in a slump. 😴",
            Self::ChuggingAlong => "just chugging along... 🚂",
            Self::LookingHot => "looking HOT! 🔥",
            Self::HotHotHot => "HOT HOT HOT!!! 🔥🥵🔥",
        }
    }
}
