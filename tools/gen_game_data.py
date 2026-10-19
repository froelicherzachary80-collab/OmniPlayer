#!/usr/bin/env python3
"""Generate the compact game-data tables OmniCore embeds.

Input: the Apache-2.0 licensed `net.minestom:data:<version>` artifact, which is
data extracted from the vanilla Minecraft server (block states, items, entity
types, map colors, tags). No Mojang code or art is copied.

Usage:
    python3 tools/gen_game_data.py <extracted data dir> services/omnicore/data
"""
import json
import os
import re
import sys

src, dst = sys.argv[1], sys.argv[2]
os.makedirs(dst, exist_ok=True)


def load(name):
    with open(os.path.join(src, name)) as f:
        return json.load(f)


BOX_RE = re.compile(r"AABB\[([-\d.E]+), ([-\d.E]+), ([-\d.E]+)\] -> \[([-\d.E]+), ([-\d.E]+), ([-\d.E]+)\]")


def parse_shape(s):
    return [[round(float(v), 5) for v in m.groups()] for m in BOX_RE.finditer(s or "")]


map_colors = {c["id"]: c["color"] for c in load("map_colors.json")}
blocks = load("block.json")
shape_index = {}
shapes = []


def shape_id(s):
    key = json.dumps(parse_shape(s))
    if key not in shape_index:
        shape_index[key] = len(shapes)
        shapes.append(json.loads(key))
    return shape_index[key]


block_rows = []
state_count = 1 + max(st["stateId"] for b in blocks.values() for st in b["states"].values())
states = [None] * state_count
for name, b in sorted(blocks.items(), key=lambda kv: kv[1]["id"]):
    block_rows.append({
        "id": b["id"],
        "name": name.removeprefix("minecraft:"),
        "hardness": b.get("hardness", 0.0),
        "requires_tool": b.get("requiresTool", False),
        "color": map_colors.get(b.get("mapColorId", 0), 0),
        "default_state": b["defaultStateId"],
        "item": (b.get("correspondingItem") or "").removeprefix("minecraft:"),
        "friction": b.get("friction", 0.6),
    })
    for props, st in b["states"].items():
        solid = st.get("solid", b.get("solid", False))
        fluid = st.get("fluid", False)
        occludes = st.get("occludes", b.get("occludes", False))
        flags = (1 if solid else 0) | (2 if fluid else 0) | (4 if occludes else 0)
        states[st["stateId"]] = [
            b["id"],
            props.strip("[]"),
            shape_id(st.get("collisionShape", b.get("collisionShape"))),
            flags,
        ]

items = load("item.json")
item_rows = []
for name, it in sorted(items.items(), key=lambda kv: kv[1]["id"]):
    comps = it.get("components", {})
    food = comps.get("minecraft:food")
    tool = comps.get("minecraft:tool")
    item_rows.append({
        "id": it["id"],
        "name": name.removeprefix("minecraft:"),
        "stack": comps.get("minecraft:max_stack_size", 64),
        "max_damage": comps.get("minecraft:max_damage", 0),
        "food": food.get("nutrition", 0) if food else 0,
        "tool_speed": (tool or {}).get("default_mining_speed", 1.0) if tool else 0,
        "block": (it.get("correspondingBlock") or "").removeprefix("minecraft:"),
    })

entities = load("entity_type.json")
entity_rows = []
for name, e in sorted(entities.items(), key=lambda kv: kv[1]["id"]):
    attrs = e.get("defaultAttributes", {})
    entity_rows.append({
        "id": e["id"],
        "name": name.removeprefix("minecraft:"),
        "width": e.get("width", 0.6),
        "height": e.get("height", 1.8),
        "kind": e.get("packetType", "BASE"),
        "max_health": attrs.get("minecraft:max_health", 0),
    })

# Resolve block tags that matter for gameplay decisions.
tags = load("tags/block.json")


def resolve(tag, seen=None):
    seen = seen or set()
    out = []
    for v in tags.get(tag, {}).get("values", []):
        if v.startswith("#"):
            if v[1:] not in seen:
                seen.add(v[1:])
                out += resolve(v[1:], seen)
        else:
            out.append(v.removeprefix("minecraft:"))
    return sorted(set(out))


wanted = ["mineable/pickaxe", "mineable/axe", "mineable/shovel", "mineable/hoe",
          "needs_stone_tool", "needs_iron_tool", "needs_diamond_tool", "logs", "leaves",
          "planks", "beds", "doors", "climbable", "fences", "walls", "coal_ores", "iron_ores",
          "diamond_ores", "gold_ores", "copper_ores", "redstone_ores", "lapis_ores", "emerald_ores",
          "dirt", "sand", "crops", "flowers", "replaceable"]
tag_rows = {t: resolve("minecraft:" + t) for t in wanted}

constants = load("constants.json")
out = {
    "version": constants["name"],
    "protocol": constants["protocol"],
    "blocks": block_rows,
    "states": states,
    "shapes": shapes,
    "items": item_rows,
    "entities": entity_rows,
    "tags": tag_rows,
}
with open(os.path.join(dst, "game_data_26_3.json"), "w") as f:
    json.dump(out, f, separators=(",", ":"))
print(f"blocks={len(block_rows)} states={state_count} shapes={len(shapes)} items={len(item_rows)} entities={len(entity_rows)}")
