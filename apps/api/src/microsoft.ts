// Microsoft → Xbox Live → Minecraft authentication using the OAuth 2.0 device
// code flow. The user signs in on Microsoft's own page (microsoft.com/link);
// OmniPlayer never sees or asks for a Microsoft password.

import { config } from "./config.ts";

const MS = process.env.OMNI_MS_AUTHORITY ?? "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const XBL = process.env.OMNI_XBL_URL ?? "https://user.auth.xboxlive.com/user/authenticate";
const XSTS = process.env.OMNI_XSTS_URL ?? "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC = process.env.OMNI_MC_SERVICES ?? "https://api.minecraftservices.com";
const SCOPE = "XboxLive.signin offline_access";

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
  message: string;
}

export interface McSession {
  ms_refresh_token: string;
  mc_access_token: string;
  mc_expires_at: number;
  profile_id: string;
  name: string;
  skin_url: string | null;
  entitled: boolean;
}

function form(data: Record<string, string>) {
  return { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: new URLSearchParams(data).toString() };
}

async function json(url: string, init: RequestInit): Promise<any> {
  const r = await fetch(url, init);
  const text = await r.text();
  let body: any = {};
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = { raw: text };
  }
  if (!r.ok) {
    const err: any = new Error(body.error_description || body.errorMessage || body.error || body.XErr ? `${body.error ?? ""} ${body.error_description ?? ""} XErr=${body.XErr ?? ""}`.trim() : `HTTP ${r.status} from ${url}`);
    err.status = r.status;
    err.body = body;
    throw err;
  }
  return body;
}

export function requireClientId(): string {
  if (!config.msClientId) {
    throw new Error("Microsoft sign-in is not configured: set OMNI_MS_CLIENT_ID to your Azure app's client ID (see docs/ACCOUNTS.md).");
  }
  return config.msClientId;
}

export async function startDeviceCode(): Promise<DeviceCode> {
  return json(`${MS}/devicecode`, form({ client_id: requireClientId(), scope: SCOPE }));
}

/** Returns tokens once the user finished signing in, or null while still pending. */
export async function pollDeviceCode(deviceCode: string): Promise<{ access_token: string; refresh_token: string } | null> {
  try {
    return await json(`${MS}/token`, form({ client_id: requireClientId(), grant_type: "urn:ietf:params:oauth:grant-type:device_code", device_code: deviceCode }));
  } catch (e: any) {
    const code = e.body?.error;
    if (code === "authorization_pending" || code === "slow_down") return null;
    throw e;
  }
}

async function refreshMs(refresh: string) {
  return json(`${MS}/token`, form({ client_id: requireClientId(), grant_type: "refresh_token", refresh_token: refresh, scope: SCOPE }));
}

/** Microsoft access token → Minecraft session (+ entitlement and profile checks). */
export async function minecraftFromMs(msAccess: string, msRefresh: string): Promise<McSession> {
  const xbl = await json(XBL, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ Properties: { AuthMethod: "RPS", SiteName: "user.auth.xboxlive.com", RpsTicket: `d=${msAccess}` }, RelyingParty: "http://auth.xboxlive.com", TokenType: "JWT" }),
  });
  const uhs = xbl.DisplayClaims?.xui?.[0]?.uhs;
  let xsts;
  try {
    xsts = await json(XSTS, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({ Properties: { SandboxId: "RETAIL", UserTokens: [xbl.Token] }, RelyingParty: "rp://api.minecraftservices.com/", TokenType: "JWT" }),
    });
  } catch (e: any) {
    const x = e.body?.XErr;
    if (x === 2148916233) throw new Error("This Microsoft account has no Xbox profile yet. Sign in once at minecraft.net to create one.");
    if (x === 2148916238) throw new Error("Child accounts must be added to a Microsoft family before they can sign in.");
    throw e;
  }
  const mc = await json(`${MC}/authentication/login_with_xbox`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ identityToken: `XBL3.0 x=${uhs};${xsts.Token}` }),
  });
  const auth = { Authorization: `Bearer ${mc.access_token}` };
  const ent = await json(`${MC}/entitlements/mcstore`, { headers: auth });
  const entitled = (ent.items ?? []).some((i: any) => i.name === "game_minecraft" || i.name === "product_minecraft");
  let profile: any = null;
  try {
    profile = await json(`${MC}/minecraft/profile`, { headers: auth });
  } catch (e: any) {
    if (e.status === 404) throw new Error("This account does not own Minecraft: Java Edition (no Java profile).");
    throw e;
  }
  const skin = (profile.skins ?? []).find((s: any) => s.state === "ACTIVE")?.url ?? null;
  const id: string = profile.id;
  const dashed = `${id.slice(0, 8)}-${id.slice(8, 12)}-${id.slice(12, 16)}-${id.slice(16, 20)}-${id.slice(20)}`;
  return {
    ms_refresh_token: msRefresh,
    mc_access_token: mc.access_token,
    mc_expires_at: Date.now() + (mc.expires_in ?? 86400) * 1000,
    profile_id: dashed,
    name: profile.name,
    skin_url: skin,
    entitled,
  };
}

/** Ensures a valid Minecraft access token, refreshing through Microsoft when needed. */
export async function ensureFresh(s: McSession): Promise<McSession> {
  if (s.mc_expires_at - Date.now() > 10 * 60 * 1000) return s;
  const t = await refreshMs(s.ms_refresh_token);
  return minecraftFromMs(t.access_token, t.refresh_token ?? s.ms_refresh_token);
}
