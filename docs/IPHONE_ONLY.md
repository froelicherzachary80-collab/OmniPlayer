# Run OmniPlayer using only your iPhone

You don't need a computer. You rent a small cloud server, paste one script into
its setup box from Safari, and then use everything from your phone:

- the OmniPlayer website (dashboard + MineLive), at `https://<your-ip>.sslip.io`,
  added to your Home Screen as an app
- **Minecraft on your iPhone** joins the same world as the AI players (Geyser
  translates Bedrock ↔ Java)
- friends on PC (Java Edition) can join too

## What it costs

The AI players, the Minecraft server and the live streams all run on the cloud
server, so it needs some power:

| Use | Server size | Typical price |
| --- | --- | --- |
| 1 Minecraft server, 2–3 AI players, 1 live stream | 4 vCPU / 8 GB RAM | ~€8–15 / month (Hetzner CPX31/CX32, DigitalOcean, Vultr) |
| 5+ AI players or 2 streams | 8 vCPU / 16 GB RAM | ~€15–30 / month |

You can delete the server whenever you want and you stop paying.
4 GB RAM servers work for a small test (the script adds swap), but expect lag.

## Step 1: create the server (about 3 minutes, in Safari)

Any provider with a "user data" / "cloud-init" box works. Hetzner Cloud is a
good example because its website works well on iPhone:

1. Sign up and open **Create server**.
2. **Image:** Ubuntu 24.04. **Type:** at least 4 vCPU / 8 GB.
3. **Firewall:** either add none, or create one that allows
   TCP 22, 80, 443, 25565–25575 and UDP 8189, 19132–19140.
4. **Cloud config / User data:** open
   [`scripts/cloud-setup.sh`](../scripts/cloud-setup.sh) on GitHub, tap **Raw**,
   select all, copy, and paste it into the box.
5. Before you create the server, edit one line at the top of the pasted text:
   `ADMIN_PASSWORD="choose-a-long-password"` (12+ characters). This is your
   dashboard password. Don't reuse a password from another site.
6. Tap **Create**. Note the server's **IPv4 address**, e.g. `203.0.113.7`.

(DigitalOcean: Create Droplet → Advanced options → *Add initialization scripts*.
Vultr: Deploy → *Cloud-Init User-Data*. Same script.)

## Step 2: wait for the first build (10–20 minutes)

The server installs Docker and builds OmniPlayer itself. Your website address is
your IP with dashes plus `.sslip.io`:

```
203.0.113.7  →  https://203-0-113-7.sslip.io
```

Open it in Safari. Until the build is done you'll see a connection error; try
again in a few minutes. The HTTPS certificate is issued automatically.

If you left the password empty, open your provider's web **Console** for the
server; the generated password is printed there (and saved in
`/root/omniplayer-password.txt`).

## Step 3: install it as an app

In Safari tap **Share → Add to Home Screen**. OmniPlayer opens full screen like
an app. Log in with your admin password.

## Step 4: make a world and play with the AI on your iPhone

1. **Play** tab → create a server. It's created as a **Paper** server with
   **iPhone players allowed**. Accept the Minecraft EULA → **Start Server**.
   The first start downloads the official server and installs Geyser + Floodgate.
2. Server page → **Players** → *iPhone / Bedrock players* → add your **Xbox
   gamertag** (the name Minecraft on iPhone signs in with).
3. **Creators** → make an AI creator → back on **Play**, pick it → **Join AI Players**.
4. Open **Minecraft on your iPhone** → Play → **Servers** → **Add Server**:
   - Server address: your IP (e.g. `203.0.113.7`)
   - Port: `19132`
   The exact address is shown on the server's Dashboard under *iPhone / Bedrock*.
5. You're in the world with the AI players. Watch their real POV streams on
   **MineLive** in the website (switch apps, or use a second device).

## Plugins and mods

Server page → **Plugins** (Paper) or **Mods** (Fabric): search Modrinth and tap
**Install**, or **Upload .jar** from the Files app. Restart the server to load them.

- Plugins (Paper): EssentialsX, LuckPerms, WorldEdit, CoreProtect, etc.
- Server-side Fabric mods (Lithium, etc.) work with everyone. Mods that add new
  blocks or items must also be installed on every player's game. Minecraft on
  iPhone can't install Java mods, and the AI players can't use them either, so
  OmniPlayer warns you when a mod is like that.

## Updating

From the provider's web console: `bash /opt/omniplayer/scripts/cloud-setup.sh`
pulls the latest version and rebuilds. Your worlds and settings are kept in
Docker volumes.

## Safety notes

- The website is public on the internet and protected by your admin password.
  Use a long one.
- New servers are **Private offline** with an allowlist. Only names you add can
  join; Bedrock players are additionally checked by Xbox sign-in (Floodgate).
- OmniPlayer never asks for your Microsoft password; Microsoft accounts use
  Microsoft's own sign-in page.
