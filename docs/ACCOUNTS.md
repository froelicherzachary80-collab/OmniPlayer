# Minecraft accounts

## Authenticated (Microsoft) accounts
OmniPlayer uses the OAuth 2.0 **device code flow**: the dashboard shows a code, you sign in on Microsoft's own page (microsoft.com/link). OmniPlayer never sees your Microsoft password. After sign-in it runs Xbox Live → XSTS → `login_with_xbox`, checks the **Java Edition entitlement** and the profile, and stores the refresh token AES-256-GCM encrypted (`OMNI_SECRET_KEY`). Tokens are refreshed before each bot start.

Setup:
1. Register an app at portal.azure.com → App registrations → *Personal Microsoft accounts*; enable *Allow public client flows*.
2. Put its client ID in `OMNI_MS_CLIENT_ID`.
3. Mojang requires new third-party apps to be **approved** before they can call Minecraft services (otherwise `login_with_xbox` returns an "Invalid app registration" error). Apply through Mojang's published app-review form.

Rules enforced: one account plays in one session at a time; accounts in use can't be reassigned; offline accounts can't be assigned to online-mode servers (compatibility check before joining).

*Status:* implemented, not exercised in the development sandbox (no Azure app, Mojang hosts blocked).

**Chat signing:** bots don't yet sign chat with player certificates, so servers created by OmniPlayer set `enforce-secure-profile=false`. On third-party online servers that enforce secure chat, bots can play but not chat.

## Offline-mode accounts
For private servers that permit offline mode. The UUID is computed exactly like vanilla (`UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`), so inventories and stats persist across sessions. Offline names are **not** verified identities: never use someone else's name. Reserve names (yours, friends') in Accounts → Reserved usernames.

## Mixed private servers
"Private Offline" servers let licensed players and offline bots play together: the manager writes `online-mode=false`, `white-list=true`, `enforce-whitelist=true`, offline-UUID allowlist entries, and invitations add friends. Mojang authentication is **not** enforced on such servers — keep them on a VPN or firewalled. For full verification create an "Online" server (every player, including bots, needs a real account).

## Skins
Authenticated accounts use their Mojang skin. Custom/generated skins (Profile & skin tab) are shown on MineLive and can be downloaded; offline players only see them in-game on Paper with a skin plugin such as SkinsRestorer.
