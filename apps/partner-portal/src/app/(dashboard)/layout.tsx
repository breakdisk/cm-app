"use client";

import { useState, type ReactNode } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import {
  LayoutDashboard,
  Target,
  DollarSign,
  Tag,
  ClipboardList,
  Settings,
  ChevronLeft,
  ChevronRight,
  LogOut,
  Zap,
  Menu,
  X,
  Users,
  PackagePlus,
} from "lucide-react";
import { cn } from "@/lib/design-system/cn";
import { NeonBadge } from "@/components/ui/neon-badge";

// ─── Types ────────────────────────────────────────────────────────────────────

interface NavItem {
  label: string;
  href: string;
  icon: React.ElementType;
}

interface DashboardLayoutProps {
  children: ReactNode;
}

// ─── Nav config ───────────────────────────────────────────────────────────────

const NAV_ITEMS: NavItem[] = [
  { label: "Overview",    href: "/",          icon: LayoutDashboard },
  { label: "New Orders",  href: "/orders",    icon: PackagePlus },
  { label: "SLA Dashboard", href: "/sla",     icon: Target },
  { label: "Payouts",     href: "/payouts",   icon: DollarSign },
  { label: "Rate Cards",  href: "/rates",     icon: Tag },
  { label: "Manifests",   href: "/manifests", icon: ClipboardList },
  { label: "Drivers",     href: "/drivers",   icon: Users },
  { label: "Settings",    href: "/settings",  icon: Settings },
];

// ─── Page title map ───────────────────────────────────────────────────────────

const PAGE_TITLE_MAP: Record<string, string> = {
  "/":          "Overview",
  "/orders":    "New Orders",
  "/sla":       "SLA Dashboard",
  "/payouts":   "Payouts",
  "/rates":     "Rate Cards",
  "/manifests": "Manifests",
  "/drivers":   "Drivers",
  "/settings":  "Settings",
};

function getPageTitle(pathname: string): string {
  if (PAGE_TITLE_MAP[pathname]) return PAGE_TITLE_MAP[pathname];
  const match = Object.keys(PAGE_TITLE_MAP)
    .filter((k) => k !== "/" && pathname.startsWith(k + "/"))
    .sort((a, b) => b.length - a.length)[0];
  return match ? PAGE_TITLE_MAP[match] : "Partner Portal";
}

// ─── Sidebar Nav Item ─────────────────────────────────────────────────────────

function NavLink({
  item,
  isActive,
  collapsed,
}: {
  item: NavItem;
  isActive: boolean;
  collapsed: boolean;
}) {
  const Icon = item.icon;

  return (
    <Link
      href={item.href}
      className={cn(
        "group relative flex items-center gap-3 rounded-lg px-3 py-2.5 transition-all duration-200",
        "text-sm font-medium",
        isActive
          ? "text-green-signal bg-green-surface"
          : "text-white/50 hover:text-white/80 hover:bg-glass-200"
      )}
      style={
        isActive
          ? {
              boxShadow:
                "inset 3px 0 0 #00FF88, 0 0 12px rgba(0,255,136,0.08)",
            }
          : undefined
      }
    >
      <Icon
        className={cn(
          "h-4 w-4 flex-shrink-0 transition-colors duration-200",
          isActive
            ? "text-green-signal"
            : "text-white/40 group-hover:text-white/70"
        )}
      />

      <AnimatePresence initial={false}>
        {!collapsed && (
          <motion.span
            initial={{ opacity: 0, width: 0 }}
            animate={{ opacity: 1, width: "auto" }}
            exit={{ opacity: 0, width: 0 }}
            transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
            className="overflow-hidden whitespace-nowrap"
          >
            {item.label}
          </motion.span>
        )}
      </AnimatePresence>

      {/* Tooltip for collapsed state */}
      {collapsed && (
        <div
          className={cn(
            "pointer-events-none absolute left-full ml-3 z-50",
            "rounded-md border border-glass-border bg-canvas-100 px-2.5 py-1.5",
            "text-xs font-medium text-white/80 whitespace-nowrap",
            "opacity-0 group-hover:opacity-100 transition-opacity duration-150",
            "shadow-glass"
          )}
        >
          {item.label}
        </div>
      )}
    </Link>
  );
}

// ─── Layout ───────────────────────────────────────────────────────────────────

export default function DashboardLayout({ children }: DashboardLayoutProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const pathname = usePathname();
  const pageTitle = getPageTitle(pathname);

  // Close mobile menu on navigation
  useState(() => { setMobileOpen(false); });

  return (
    <div className="flex min-h-screen bg-canvas font-sans antialiased">
      {/* ── Mobile backdrop ──────────────────────────────────────────────── */}
      <AnimatePresence>
        {mobileOpen && (
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm md:hidden"
            onClick={() => setMobileOpen(false)}
          />
        )}
      </AnimatePresence>

      {/* ── Sidebar ─────────────────────────────────────────────────────── */}
      <aside
        className={cn(
          "flex flex-shrink-0 flex-col overflow-hidden",
          "transition-all duration-[250ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
          collapsed ? "w-16" : "w-60",
          "fixed inset-y-0 left-0 z-50",
          mobileOpen ? "translate-x-0" : "-translate-x-full",
          "md:relative md:inset-auto md:z-auto md:translate-x-0",
        )}
        style={{
          background: "rgba(5, 8, 16, 0.97)",
          backdropFilter: "blur(24px)",
          WebkitBackdropFilter: "blur(24px)",
          borderRight: "1px solid rgba(255, 255, 255, 0.06)",
        }}
      >
        {/* ── Carrier logo + PARTNER PORTAL badge ───────────────────────── */}
        <div
          className={cn(
            "flex h-16 flex-shrink-0 flex-col justify-center border-b border-glass-border",
            collapsed ? "items-center px-0" : "px-5"
          )}
        >
          <div className="flex items-center gap-2.5 min-w-0">
            {/* Icon mark */}
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg"
              style={{
                background: "linear-gradient(135deg, #00FF88 0%, #00B8D9 100%)",
                boxShadow: "0 0 14px rgba(0,255,136,0.35)",
              }}
            >
              <Zap className="h-4 w-4 text-canvas" strokeWidth={2.5} />
            </div>

            <AnimatePresence initial={false}>
              {!collapsed && (
                <motion.div
                  initial={{ opacity: 0, width: 0 }}
                  animate={{ opacity: 1, width: "auto" }}
                  exit={{ opacity: 0, width: 0 }}
                  transition={{ duration: 0.2 }}
                  className="overflow-hidden"
                >
                  <span className="whitespace-nowrap font-heading text-sm font-bold tracking-tight text-white">
                    FastShip Co.
                  </span>
                  <div className="mt-0.5">
                    <NeonBadge variant="green" className="text-2xs">
                      Partner Portal
                    </NeonBadge>
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>

        {/* ── Navigation ────────────────────────────────────────────────── */}
        <nav className="flex flex-1 flex-col gap-1 overflow-y-auto overflow-x-hidden p-3">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.href}
              item={item}
              isActive={
                item.href === "/"
                  ? pathname === "/"
                  : pathname === item.href || pathname.startsWith(item.href + "/")
              }
              collapsed={collapsed}
            />
          ))}
        </nav>

        {/* ── User section ──────────────────────────────────────────────── */}
        <div
          className={cn(
            "flex-shrink-0 border-t border-glass-border p-3",
            collapsed ? "flex flex-col items-center gap-2" : "space-y-2"
          )}
        >
          <div
            className={cn(
              "flex items-center gap-3 rounded-lg px-2 py-2",
              "bg-glass-100 transition-colors hover:bg-glass-200"
            )}
          >
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-xs font-bold text-canvas"
              style={{ background: "linear-gradient(135deg, #00FF88, #00B8D9)" }}
            >
              RC
            </div>
            <AnimatePresence initial={false}>
              {!collapsed && (
                <motion.div
                  initial={{ opacity: 0, width: 0 }}
                  animate={{ opacity: 1, width: "auto" }}
                  exit={{ opacity: 0, width: 0 }}
                  transition={{ duration: 0.2 }}
                  className="min-w-0 flex-1 overflow-hidden"
                >
                  <p className="truncate whitespace-nowrap text-xs font-medium text-white/80">
                    Roberto Cruz
                  </p>
                  <p className="truncate whitespace-nowrap text-2xs text-white/40 font-mono">
                    Partner Admin
                  </p>
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          <button
            className={cn(
              "group flex w-full items-center gap-3 rounded-lg px-3 py-2",
              "text-xs text-white/40 transition-all hover:bg-red-surface hover:text-red-signal",
              collapsed && "justify-center"
            )}
          >
            <LogOut className="h-3.5 w-3.5 flex-shrink-0" />
            <AnimatePresence initial={false}>
              {!collapsed && (
                <motion.span
                  initial={{ opacity: 0, width: 0 }}
                  animate={{ opacity: 1, width: "auto" }}
                  exit={{ opacity: 0, width: 0 }}
                  transition={{ duration: 0.2 }}
                  className="overflow-hidden whitespace-nowrap"
                >
                  Sign out
                </motion.span>
              )}
            </AnimatePresence>
          </button>
        </div>

        {/* ── Collapse toggle (desktop only) ────────────────────────────── */}
        <button
          onClick={() => setCollapsed((c) => !c)}
          className={cn(
            "absolute -right-3 top-[4.5rem] z-20 hidden md:flex",
            "h-6 w-6 items-center justify-center rounded-full",
            "border border-glass-border-bright bg-canvas-100 text-white/60",
            "transition-all hover:border-green-signal/50 hover:text-green-signal",
            "hover:shadow-[0_0_8px_rgba(0,255,136,0.4)]"
          )}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? (
            <ChevronRight className="h-3 w-3" />
          ) : (
            <ChevronLeft className="h-3 w-3" />
          )}
        </button>
      </aside>

      {/* ── Main content area ───────────────────────────────────────────── */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* ── Top header ────────────────────────────────────────────────── */}
        <header
          className="flex h-16 flex-shrink-0 items-center justify-between border-b border-glass-border px-4 md:px-6"
          style={{
            background: "rgba(5, 8, 16, 0.7)",
            backdropFilter: "blur(12px)",
            WebkitBackdropFilter: "blur(12px)",
          }}
        >
          <div className="flex items-center gap-3">
            <button
              className="flex h-9 w-9 items-center justify-center rounded-lg border border-glass-border bg-glass-100 text-white/60 transition-all hover:bg-glass-200 hover:text-white/80 md:hidden"
              onClick={() => setMobileOpen((o) => !o)}
              aria-label="Toggle navigation"
            >
              {mobileOpen ? <X className="h-4 w-4" /> : <Menu className="h-4 w-4" />}
            </button>
            <motion.h1
              key={pageTitle}
              initial={{ opacity: 0, y: -6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
              className="font-heading text-lg font-semibold text-white"
            >
              {pageTitle}
            </motion.h1>
          </div>

          {/* Right — SLA quick status */}
          <div className="flex items-center gap-3">
            <div className="hidden items-center gap-2 sm:flex">
              <span className="text-xs text-white/40">SLA Rate</span>
              <span
                className="font-mono text-sm font-bold text-green-signal"
                style={{ textShadow: "0 0 8px rgba(0,255,136,0.4)" }}
              >
                96.8%
              </span>
            </div>
            <div className="h-4 w-px bg-glass-border hidden sm:block" />
            <NeonBadge variant="green" dot pulse>
              Active
            </NeonBadge>
          </div>
        </header>

        {/* ── Page content ──────────────────────────────────────────────── */}
        <main className="flex-1 overflow-auto bg-canvas p-4 md:p-6">
          <motion.div
            key={pathname}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          >
            {children}
          </motion.div>
        </main>
      </div>
    </div>
  );
}
