// Generated from the 26.3 (protocol 777) packet registration order.
// Do not edit by hand; regenerate with tools/gen_packet_ids.py.

pub mod client_handshake {
    pub const HANDSHAKE: i32 = 0x00;
}

pub mod client_status {
    pub const STATUS_REQUEST: i32 = 0x00;
    pub const PING_REQUEST: i32 = 0x01;
}

pub mod client_login {
    pub const LOGIN_START: i32 = 0x00;
    pub const ENCRYPTION_RESPONSE: i32 = 0x01;
    pub const LOGIN_PLUGIN_RESPONSE: i32 = 0x02;
    pub const LOGIN_ACKNOWLEDGED: i32 = 0x03;
    pub const COOKIE_RESPONSE: i32 = 0x04;
}

pub mod client_configuration {
    pub const SETTINGS: i32 = 0x00;
    pub const COOKIE_RESPONSE: i32 = 0x01;
    pub const PLUGIN_MESSAGE: i32 = 0x02;
    pub const FINISH_CONFIGURATION: i32 = 0x03;
    pub const KEEP_ALIVE: i32 = 0x04;
    pub const PONG: i32 = 0x05;
    pub const RESOURCE_PACK_STATUS: i32 = 0x06;
    pub const SELECT_KNOWN_PACKS: i32 = 0x07;
    pub const CUSTOM_CLICK_ACTION: i32 = 0x08;
    pub const ACCEPT_CODE_OF_CONDUCT: i32 = 0x09;
}

pub mod client_play {
    pub const TELEPORT_CONFIRM: i32 = 0x00;
    pub const ATTACK: i32 = 0x01;
    pub const QUERY_BLOCK_NBT: i32 = 0x02;
    pub const SELECT_BUNDLE_ITEM: i32 = 0x03;
    pub const CHANGE_DIFFICULTY: i32 = 0x04;
    pub const CHANGE_GAME_MODE: i32 = 0x05;
    pub const CHAT_ACK: i32 = 0x06;
    pub const COMMAND_CHAT: i32 = 0x07;
    pub const SIGNED_COMMAND_CHAT: i32 = 0x08;
    pub const CHAT_MESSAGE: i32 = 0x09;
    pub const CHAT_SESSION_UPDATE: i32 = 0x0A;
    pub const CHUNK_BATCH_RECEIVED: i32 = 0x0B;
    pub const STATUS: i32 = 0x0C;
    pub const TICK_END: i32 = 0x0D;
    pub const SETTINGS: i32 = 0x0E;
    pub const TAB_COMPLETE: i32 = 0x0F;
    pub const CONFIGURATION_ACK: i32 = 0x10;
    pub const CLICK_WINDOW_BUTTON: i32 = 0x11;
    pub const CLICK_WINDOW: i32 = 0x12;
    pub const CLOSE_WINDOW: i32 = 0x13;
    pub const WINDOW_SLOT_STATE: i32 = 0x14;
    pub const COOKIE_RESPONSE: i32 = 0x15;
    pub const PLUGIN_MESSAGE: i32 = 0x16;
    pub const DEBUG_SUBSCRIPTION_REQUEST: i32 = 0x17;
    pub const EDIT_BOOK: i32 = 0x18;
    pub const QUERY_ENTITY_NBT: i32 = 0x19;
    pub const INTERACT_ENTITY: i32 = 0x1A;
    pub const GENERATE_STRUCTURE: i32 = 0x1B;
    pub const KEEP_ALIVE: i32 = 0x1C;
    pub const LOCK_DIFFICULTY: i32 = 0x1D;
    pub const PLAYER_POSITION: i32 = 0x1E;
    pub const PLAYER_POSITION_AND_ROTATION: i32 = 0x1F;
    pub const PLAYER_ROTATION: i32 = 0x20;
    pub const PLAYER_POSITION_STATUS: i32 = 0x21;
    pub const VEHICLE_MOVE: i32 = 0x22;
    pub const STEER_BOAT: i32 = 0x23;
    pub const PICK_ITEM_FROM_BLOCK: i32 = 0x24;
    pub const PICK_ITEM_FROM_ENTITY: i32 = 0x25;
    pub const PING_REQUEST: i32 = 0x26;
    pub const PLACE_RECIPE: i32 = 0x27;
    pub const PLAYER_ABILITIES: i32 = 0x28;
    pub const PLAYER_ACTION: i32 = 0x29;
    pub const ENTITY_ACTION: i32 = 0x2A;
    pub const INPUT: i32 = 0x2B;
    pub const PLAYER_LOADED: i32 = 0x2C;
    pub const PONG: i32 = 0x2D;
    pub const PUNCH: i32 = 0x2E;
    pub const SET_RECIPE_BOOK_STATE: i32 = 0x2F;
    pub const RECIPE_BOOK_SEEN_RECIPE: i32 = 0x30;
    pub const NAME_ITEM: i32 = 0x31;
    pub const RESOURCE_PACK_STATUS: i32 = 0x32;
    pub const ADVANCEMENT_TAB: i32 = 0x33;
    pub const SELECT_TRADE: i32 = 0x34;
    pub const SET_BEACON_EFFECT: i32 = 0x35;
    pub const HELD_ITEM_CHANGE: i32 = 0x36;
    pub const UPDATE_COMMAND_BLOCK: i32 = 0x37;
    pub const UPDATE_COMMAND_BLOCK_MINECART: i32 = 0x38;
    pub const CREATIVE_INVENTORY_ACTION: i32 = 0x39;
    pub const SET_GAME_RULES: i32 = 0x3A;
    pub const UPDATE_JIGSAW_BLOCK: i32 = 0x3B;
    pub const UPDATE_STRUCTURE_BLOCK: i32 = 0x3C;
    pub const SET_TEST_BLOCK: i32 = 0x3D;
    pub const UPDATE_SIGN: i32 = 0x3E;
    pub const SPECTATOR_ACTION: i32 = 0x3F;
    pub const TELEPORT_TO_ENTITY: i32 = 0x40;
    pub const TEST_INSTANCE_BLOCK_ACTION: i32 = 0x41;
    pub const PLAYER_BLOCK_PLACEMENT: i32 = 0x42;
    pub const USE_ITEM: i32 = 0x43;
    pub const CUSTOM_CLICK_ACTION: i32 = 0x44;
}

pub mod server_handshake {
}

pub mod server_status {
    pub const RESPONSE: i32 = 0x00;
    pub const PING_RESPONSE: i32 = 0x01;
}

pub mod server_login {
    pub const LOGIN_DISCONNECT: i32 = 0x00;
    pub const ENCRYPTION_REQUEST: i32 = 0x01;
    pub const LOGIN_SUCCESS: i32 = 0x02;
    pub const SET_COMPRESSION: i32 = 0x03;
    pub const LOGIN_PLUGIN_REQUEST: i32 = 0x04;
    pub const COOKIE_REQUEST: i32 = 0x05;
}

pub mod server_configuration {
    pub const COOKIE_REQUEST: i32 = 0x00;
    pub const PLUGIN_MESSAGE: i32 = 0x01;
    pub const DISCONNECT: i32 = 0x02;
    pub const FINISH_CONFIGURATION: i32 = 0x03;
    pub const KEEP_ALIVE: i32 = 0x04;
    pub const PING: i32 = 0x05;
    pub const RESET_CHAT: i32 = 0x06;
    pub const REGISTRY_DATA: i32 = 0x07;
    pub const RESOURCE_PACK_POP: i32 = 0x08;
    pub const RESOURCE_PACK_PUSH: i32 = 0x09;
    pub const POST_EFFECTS: i32 = 0x0A;
    pub const COOKIE_STORE: i32 = 0x0B;
    pub const TRANSFER: i32 = 0x0C;
    pub const UPDATE_ENABLED_FEATURES: i32 = 0x0D;
    pub const TAGS: i32 = 0x0E;
    pub const SELECT_KNOWN_PACKS: i32 = 0x0F;
    pub const CUSTOM_REPORT_DETAILS: i32 = 0x10;
    pub const SERVER_LINKS: i32 = 0x11;
    pub const CLEAR_DIALOG: i32 = 0x12;
    pub const SHOW_DIALOG: i32 = 0x13;
    pub const CODE_OF_CONDUCT: i32 = 0x14;
}

pub mod server_play {
    pub const BUNDLE: i32 = 0x00;
    pub const SPAWN_ENTITY: i32 = 0x01;
    pub const ENTITY_ANIMATION: i32 = 0x02;
    pub const STATISTICS: i32 = 0x03;
    pub const ACKNOWLEDGE_BLOCK_CHANGE: i32 = 0x04;
    pub const BLOCK_BREAK_ANIMATION: i32 = 0x05;
    pub const BLOCK_ENTITY_DATA: i32 = 0x06;
    pub const BLOCK_ACTION: i32 = 0x07;
    pub const BLOCK_CHANGE: i32 = 0x08;
    pub const BOSS_BAR: i32 = 0x09;
    pub const SERVER_DIFFICULTY: i32 = 0x0A;
    pub const CHUNK_BATCH_FINISHED: i32 = 0x0B;
    pub const CHUNK_BATCH_START: i32 = 0x0C;
    pub const CHUNK_BIOMES: i32 = 0x0D;
    pub const CLEAR_TITLES: i32 = 0x0E;
    pub const TAB_COMPLETE: i32 = 0x0F;
    pub const DECLARE_COMMANDS: i32 = 0x10;
    pub const CLOSE_WINDOW: i32 = 0x11;
    pub const WINDOW_ITEMS: i32 = 0x12;
    pub const WINDOW_PROPERTY: i32 = 0x13;
    pub const SET_SLOT: i32 = 0x14;
    pub const COOKIE_REQUEST: i32 = 0x15;
    pub const SET_COOLDOWN: i32 = 0x16;
    pub const CUSTOM_CHAT_COMPLETION: i32 = 0x17;
    pub const PLUGIN_MESSAGE: i32 = 0x18;
    pub const DAMAGE_EVENT: i32 = 0x19;
    pub const DEBUG_BLOCK_VALUE: i32 = 0x1A;
    pub const DEBUG_CHUNK_VALUE: i32 = 0x1B;
    pub const DEBUG_ENTITY_VALUE: i32 = 0x1C;
    pub const DEBUG_EVENT: i32 = 0x1D;
    pub const DEBUG_SAMPLE: i32 = 0x1E;
    pub const DELETE_CHAT: i32 = 0x1F;
    pub const DISCONNECT: i32 = 0x20;
    pub const DISGUISED_CHAT: i32 = 0x21;
    pub const ENTITY_STATUS: i32 = 0x22;
    pub const ENTITY_POSITION_SYNC: i32 = 0x23;
    pub const EXPLOSION: i32 = 0x24;
    pub const SET_TRANSIENT_BLOCK: i32 = 0x25;
    pub const UNLOAD_CHUNK: i32 = 0x26;
    pub const CHANGE_GAME_STATE: i32 = 0x27;
    pub const GAME_RULE_VALUES: i32 = 0x28;
    pub const GAME_TEST_HIGHLIGHT_POS: i32 = 0x29;
    pub const OPEN_HORSE_WINDOW: i32 = 0x2A;
    pub const HIT_ANIMATION: i32 = 0x2B;
    pub const INITIALIZE_WORLD_BORDER: i32 = 0x2C;
    pub const KEEP_ALIVE: i32 = 0x2D;
    pub const CHUNK_DATA: i32 = 0x2E;
    pub const WORLD_EVENT: i32 = 0x2F;
    pub const PARTICLE: i32 = 0x30;
    pub const UPDATE_LIGHT: i32 = 0x31;
    pub const JOIN_GAME: i32 = 0x32;
    pub const LOW_DISK_SPACE_WARNING: i32 = 0x33;
    pub const MAP_DATA: i32 = 0x34;
    pub const TRADE_LIST: i32 = 0x35;
    pub const ENTITY_POSITION: i32 = 0x36;
    pub const ENTITY_POSITION_AND_ROTATION: i32 = 0x37;
    pub const MOVE_MINECART: i32 = 0x38;
    pub const ENTITY_ROTATION: i32 = 0x39;
    pub const VEHICLE_MOVE: i32 = 0x3A;
    pub const OPEN_BOOK: i32 = 0x3B;
    pub const OPEN_WINDOW: i32 = 0x3C;
    pub const OPEN_SIGN_EDITOR: i32 = 0x3D;
    pub const PING: i32 = 0x3E;
    pub const PING_RESPONSE: i32 = 0x3F;
    pub const PLACE_GHOST_RECIPE: i32 = 0x40;
    pub const PLAYER_ABILITIES: i32 = 0x41;
    pub const PLAYER_CHAT_MESSAGE: i32 = 0x42;
    pub const END_COMBAT_EVENT: i32 = 0x43;
    pub const ENTER_COMBAT_EVENT: i32 = 0x44;
    pub const DEATH_COMBAT_EVENT: i32 = 0x45;
    pub const PLAYER_INFO_REMOVE: i32 = 0x46;
    pub const PLAYER_INFO_UPDATE: i32 = 0x47;
    pub const FACE_PLAYER: i32 = 0x48;
    pub const PLAYER_POSITION_AND_LOOK: i32 = 0x49;
    pub const PLAYER_ROTATION: i32 = 0x4A;
    pub const RECIPE_BOOK_ADD: i32 = 0x4B;
    pub const RECIPE_BOOK_REMOVE: i32 = 0x4C;
    pub const RECIPE_BOOK_SETTINGS: i32 = 0x4D;
    pub const DESTROY_ENTITIES: i32 = 0x4E;
    pub const REMOVE_ENTITY_EFFECT: i32 = 0x4F;
    pub const RESET_SCORE: i32 = 0x50;
    pub const RESOURCE_PACK_POP: i32 = 0x51;
    pub const RESOURCE_PACK_PUSH: i32 = 0x52;
    pub const POST_EFFECTS: i32 = 0x53;
    pub const RESPAWN: i32 = 0x54;
    pub const ENTITY_HEAD_LOOK: i32 = 0x55;
    pub const MULTI_BLOCK_CHANGE: i32 = 0x56;
    pub const SELECT_ADVANCEMENT_TAB: i32 = 0x57;
    pub const SERVER_DATA: i32 = 0x58;
    pub const ACTION_BAR: i32 = 0x59;
    pub const WORLD_BORDER_CENTER: i32 = 0x5A;
    pub const WORLD_BORDER_LERP_SIZE: i32 = 0x5B;
    pub const WORLD_BORDER_SIZE: i32 = 0x5C;
    pub const WORLD_BORDER_WARNING_DELAY: i32 = 0x5D;
    pub const WORLD_BORDER_WARNING_REACH: i32 = 0x5E;
    pub const CAMERA: i32 = 0x5F;
    pub const UPDATE_VIEW_POSITION: i32 = 0x60;
    pub const UPDATE_VIEW_DISTANCE: i32 = 0x61;
    pub const SET_CURSOR_ITEM: i32 = 0x62;
    pub const SPAWN_POSITION: i32 = 0x63;
    pub const DISPLAY_SCOREBOARD: i32 = 0x64;
    pub const ENTITY_META_DATA: i32 = 0x65;
    pub const ATTACH_ENTITY: i32 = 0x66;
    pub const ENTITY_VELOCITY: i32 = 0x67;
    pub const ENTITY_EQUIPMENT: i32 = 0x68;
    pub const SET_EXPERIENCE: i32 = 0x69;
    pub const UPDATE_HEALTH: i32 = 0x6A;
    pub const HELD_ITEM_CHANGE: i32 = 0x6B;
    pub const SCOREBOARD_OBJECTIVE: i32 = 0x6C;
    pub const SET_PASSENGERS: i32 = 0x6D;
    pub const SET_PLAYER_INVENTORY_SLOT: i32 = 0x6E;
    pub const TEAMS: i32 = 0x6F;
    pub const UPDATE_SCORE: i32 = 0x70;
    pub const UPDATE_SIMULATION_DISTANCE: i32 = 0x71;
    pub const SET_TITLE_SUB_TITLE: i32 = 0x72;
    pub const SET_TIME: i32 = 0x73;
    pub const SET_TITLE_TEXT: i32 = 0x74;
    pub const SET_TITLE_TIME: i32 = 0x75;
    pub const ENTITY_SOUND_EFFECT: i32 = 0x76;
    pub const SOUND_EFFECT: i32 = 0x77;
    pub const START_CONFIGURATION: i32 = 0x78;
    pub const STOP_SOUND: i32 = 0x79;
    pub const COOKIE_STORE: i32 = 0x7A;
    pub const SWING_ANIMATION: i32 = 0x7B;
    pub const SYSTEM_CHAT: i32 = 0x7C;
    pub const PLAYER_LIST_HEADER_AND_FOOTER: i32 = 0x7D;
    pub const NBT_QUERY_RESPONSE: i32 = 0x7E;
    pub const COLLECT_ITEM: i32 = 0x7F;
    pub const ENTITY_TELEPORT: i32 = 0x80;
    pub const TEST_INSTANCE_BLOCK_STATUS: i32 = 0x81;
    pub const SET_TICK_STATE: i32 = 0x82;
    pub const TICK_STEP: i32 = 0x83;
    pub const TRANSFER: i32 = 0x84;
    pub const ADVANCEMENTS: i32 = 0x85;
    pub const ENTITY_ATTRIBUTES: i32 = 0x86;
    pub const ENTITY_EFFECT: i32 = 0x87;
    pub const DECLARE_RECIPES: i32 = 0x88;
    pub const TAGS: i32 = 0x89;
    pub const PROJECTILE_POWER: i32 = 0x8A;
    pub const CUSTOM_REPORT_DETAILS: i32 = 0x8B;
    pub const SERVER_LINKS: i32 = 0x8C;
    pub const TRACKED_WAYPOINT: i32 = 0x8D;
    pub const CLEAR_DIALOG: i32 = 0x8E;
    pub const SHOW_DIALOG: i32 = 0x8F;
}

