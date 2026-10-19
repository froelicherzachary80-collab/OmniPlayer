import re,sys
src=open(sys.argv[1]).read()
def snake(n):
    n=re.sub(r'Packet$','',n)
    n=re.sub(r'^Client','',n)
    s=re.sub(r'(?<!^)(?=[A-Z])','_',n).upper()
    return s
out=["// Generated from the 26.3 (protocol 777) packet registration order.","// Do not edit by hand; regenerate with tools/gen_packet_ids.py.",""]
for m in re.finditer(r'static final PacketRegistry<[^>]+> (\w+) = (?:PacketRegistry\.<[^>]+>)?registry\(ConnectionState\.\w+, PacketRegistry\.ConnectionSide\.\w+(.*?)\);',src,re.S):
    name,body=m.group(1),m.group(2)
    entries=re.findall(r'entry\((\w+)\.class',body)
    out.append(f"pub mod {name.lower()} {{")
    seen=set()
    for i,e in enumerate(entries):
        c=snake(e)
        if c in seen: c=c+"_2"
        seen.add(c)
        out.append(f"    pub const {c}: i32 = 0x{i:02X};")
    out.append("}\n")
print("\n".join(out))
