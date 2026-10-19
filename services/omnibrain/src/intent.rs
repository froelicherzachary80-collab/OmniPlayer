//! Understanding chat requests from players ("follow me", "help me mine diamonds"...).

#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Follow,
    Come,
    Stop,
    BuildHouse,
    MineDiamonds,
    ProtectBase,
    GoNether,
    FightMe,
    GiveFood,
    StartSmp,
    CollectWood,
    Explore,
    Craft(String),
    Greeting,
    Question(String),
    SetHome,
    None,
}

fn has(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| t.contains(w))
}

pub fn parse(text: &str) -> Intent {
    let t = text.to_lowercase();
    let t = t.trim();
    if has(t, &["stop", "wait here", "stay here", "hold on", "cancel"]) && !t.contains("don't stop") {
        return Intent::Stop;
    }
    if has(t, &["follow me", "follow", "come with me", "let's go together"]) {
        return Intent::Follow;
    }
    if has(t, &["come here", "come to me", "over here", "tp to me", "where i am"]) {
        return Intent::Come;
    }
    if has(t, &["build a house", "build house", "make a house", "build a base", "build us a house", "build a shelter"]) {
        return Intent::BuildHouse;
    }
    if has(t, &["mine diamonds", "find diamonds", "get diamonds", "diamond mining", "mine for diamonds"]) {
        return Intent::MineDiamonds;
    }
    if has(t, &["protect", "guard", "defend"]) {
        return Intent::ProtectBase;
    }
    if t.contains("nether") && has(t, &["go", "let's", "lets", "portal"]) {
        return Intent::GoNether;
    }
    if has(t, &["fight me", "1v1", "pvp me", "duel"]) {
        return Intent::FightMe;
    }
    if has(t, &["give me food", "give me some food", "some food", "i'm hungry", "im hungry", "share food"]) {
        return Intent::GiveFood;
    }
    if t.contains("smp") && has(t, &["create", "start", "make", "let's", "lets"]) {
        return Intent::StartSmp;
    }
    if has(t, &["get wood", "chop", "collect wood", "gather wood", "cut trees", "punch trees"]) {
        return Intent::CollectWood;
    }
    if has(t, &["explore", "look around", "scout"]) {
        return Intent::Explore;
    }
    if has(t, &["set home", "this is home", "our base is here", "base here"]) {
        return Intent::SetHome;
    }
    if let Some(rest) = t.strip_prefix("craft ").or_else(|| t.split_once(" craft ").map(|(_, r)| r)) {
        return Intent::Craft(rest.trim().replace(' ', "_").trim_start_matches("a_").to_string());
    }
    if has(t, &["hello", "hi ", "hey", "yo ", "sup"]) || t == "hi" || t == "yo" {
        return Intent::Greeting;
    }
    if t.ends_with('?') || has(t, &["how many", "where are", "what are you", "do you", "do we", "can you", "are you"]) {
        return Intent::Question(t.to_string());
    }
    Intent::None
}

/// Whether a chat message is addressed to this bot.
pub fn addressed_to(text: &str, my_name: &str, aliases: &[String]) -> bool {
    let t = text.to_lowercase();
    let n = my_name.to_lowercase();
    t.contains(&n) || aliases.iter().any(|a| !a.is_empty() && t.contains(&a.to_lowercase())) || t.starts_with("everyone") || t.starts_with("all bots") || t.starts_with("guys")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests() {
        assert_eq!(parse("Follow me."), Intent::Follow);
        assert_eq!(parse("Let's build a house"), Intent::BuildHouse);
        assert_eq!(parse("Help me mine diamonds"), Intent::MineDiamonds);
        assert_eq!(parse("Protect our base"), Intent::ProtectBase);
        assert_eq!(parse("Let's go to the Nether"), Intent::GoNether);
        assert_eq!(parse("Come fight me"), Intent::FightMe);
        assert_eq!(parse("Can you give me some food?"), Intent::GiveFood);
        assert_eq!(parse("Let's create a Minecraft SMP"), Intent::StartSmp);
        assert_eq!(parse("stop"), Intent::Stop);
        assert!(matches!(parse("Do we have enough diamonds for armor?"), Intent::Question(_)));
        assert_eq!(parse("craft wooden pickaxe"), Intent::Craft("wooden_pickaxe".into()));
    }
}
