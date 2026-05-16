import { NextRequest, NextResponse } from "next/server";
import { COOKIE_LOS_ACCESS } from "@/lib/auth/los-cookies";

function decodeJwtPayload(token: string): Record<string, unknown> | null {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return null;
    const padded = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const json = Buffer.from(padded, "base64").toString("utf-8");
    return JSON.parse(json) as Record<string, unknown>;
  } catch {
    return null;
  }
}

export async function GET(req: NextRequest) {
  const token = req.cookies.get(COOKIE_LOS_ACCESS)?.value;
  if (!token) {
    return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
  }
  const claims = decodeJwtPayload(token);
  if (!claims) {
    return NextResponse.json({ error: "Invalid token" }, { status: 401 });
  }
  return NextResponse.json({
    sub:         claims.sub         ?? null,
    email:       claims.email       ?? null,
    first_name:  claims.first_name  ?? null,
    last_name:   claims.last_name   ?? null,
    name:        claims.name        ?? null,
    roles:       Array.isArray(claims.roles)       ? claims.roles       : [],
    permissions: Array.isArray(claims.permissions) ? claims.permissions : [],
    tenant_id:   claims.tenant_id   ?? null,
  });
}
