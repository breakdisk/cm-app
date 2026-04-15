"use client";

import { useState, useEffect, type ReactNode } from "react";
import { usePathname, useRouter } from "next/navigation";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import {
  LayoutDashboard,
  Package,
  Megaphone,
  BarChart3,
  CreditCard,
  Truck,
  Settings,
  Bell,
  Search,
  ChevronLeft,
  ChevronRight,
  LogOut,
  Zap,
  Menu,
  X,
} from "lucide-react";
import { cn } from "@/lib/design-system/cn";

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
  { label: "Overview",  href: "/",          icon: LayoutDashboard },
  { label: "Shipments", href: "/shipments",  icon: Package },
  { label: "Campaigns", href: "/campaigns",  icon: Megaphone },
  { label: "Analytics", href: "/analytics",  icon: BarChart3 },
  { label: "Billing",   href: "/billing",    icon: CreditCard },
  { label: "Fleet",     href: "/fleet",      icon: Truck },
  { label: "Settings",  href: "/settings",   icon: Settings },
];

// ─── Page title map ───────────────────────────────────────────────────────────

const PAGE_TITLE_MAP: Record<string, string> = {
  "/":           "Overview",
  "/shipments":  "Shipments",
  "/campaigns":  "Campaigns",
  "/analytics":  "Analytics",
  "/billing":    "Billing",
  "/fleet":      "Fleet",
  "/settings":   "Settings",
};

function getPageTitle(pathname: string): string {
  // exact match first
  if (PAGE_TITLE_MAP[pathname]) return PAGE_TITLE_MAP[pathname];
  // prefix match for nested routes
  const match = Object.keys(PAGE_TITLE_MAP)
    .filter((k) => k !== "/" && pathname.startsWith(k))
    .sort((a, b) => b.length - a.length)[0];
  return match ? PAGE_TITLE_MAP[match] : "Dashboard";
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
          ? "text-cyan-neon bg-cyan-surface"
          : "text-white/50 hover:text-white/80 hover:bg-glass-200"
      )}
      style={
        isActive
          ? {
              boxShadow: "inset 3px 0 0 #00E5FF, 0 0 12px rgba(0,229,255,0.08)",
            }
          : undefined
      }
    >
      {/* Left border accent — shown via boxShadow above */}
      <Icon
        className={cn(
          "h-4 w-4 flex-shrink-0 transition-colors duration-200",
          isActive ? "text-cyan-neon" : "text-white/40 group-hover:text-white/70"
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
  const router = useRouter();
  const pageTitle = getPageTitle(pathname);

  useEffect(() => {
    if (typeof window === "undefined") return;
    if (!localStorage.getItem("access_token")) {
      router.replace("/login");
    }
  }, [router]);

  function handleSignOut() {
    if (typeof window !== "undefined") localStorage.removeItem("access_token");
    router.replace("/login");
  }

  return (
    <div className="flex min-h-screen bg-canvas font-sans antialiased">

      {/* ── Mobile overlay backdrop ──────────────────────────────────────── */}
      <AnimatePresence>
        {mobileOpen && (
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-40 bg-black/70 backdrop-blur-sm md:hidden"
            onClick={() => setMobileOpen(false)}
          />
        )}
      </AnimatePresence>

      {/* ── Sidebar ─────────────────────────────────────────────────────── */}
      {/* Pure CSS aside — avoids Framer Motion inline-style transform conflict */}
      <aside
        className={cn(
          "flex flex-shrink-0 flex-col overflow-hidden",
          "transition-all duration-[250ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
          // Width: CSS classes (no Framer Motion)
          collapsed ? "w-16" : "w-60",
          // Mobile: fixed overlay, slide in/out via translate
          "fixed inset-y-0 left-0 z-50",
          mobileOpen ? "translate-x-0" : "-translate-x-full",
          // Desktop: part of flow, always visible
          "md:relative md:inset-auto md:z-auto md:translate-x-0",
        )}
        style={{
          background: "rgba(5, 8, 16, 0.97)",
          backdropFilter: "blur(24px)",
          WebkitBackdropFilter: "blur(24px)",
          borderRight: "1px solid rgba(255, 255, 255, 0.06)",
        }}
      >
        {/* Mobile close button */}
        <button
          onClick={() => setMobileOpen(false)}
          className="absolute right-3 top-4 z-10 flex h-7 w-7 items-center justify-center rounded-lg border border-glass-border bg-glass-200 text-white/50 hover:text-white md:hidden"
          aria-label="Close menu"
        >
          <X className="h-3.5 w-3.5" />
        </button>
        {/* ── Logo ──────────────────────────────────────────────────────── */}
        <div
          className={cn(
            "flex h-16 flex-shrink-0 items-center border-b border-glass-border",
            collapsed ? "justify-center px-0" : "px-5"
          )}
        >
          <div className="flex items-center gap-2.5 min-w-0">
            {/* Icon mark */}
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg"
              style={{
                background: "linear-gradient(135deg, #00E5FF 0%, #A855F7 100%)",
                boxShadow: "0 0 14px rgba(0,229,255,0.35)",
              }}
            >
              <Zap className="h-4 w-4 text-white" strokeWidth={2.5} />
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
                    LogisticOS
                  </span>
                  <p className="whitespace-nowrap text-2xs text-white/30 font-mono uppercase tracking-widest">
                    Merchant
                  </p>
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
            {/* Avatar */}
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-xs font-bold text-white"
              style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
            >
              JD
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
                    Juan Dela Cruz
                  </p>
                  <p className="truncate whitespace-nowrap text-2xs text-white/40 font-mono">
                    Shopify PH
                  </p>
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          {/* Sign out */}
          <button
            onClick={handleSignOut}
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
            "absolute -right-3 top-[4.5rem] z-20",
            "hidden md:flex h-6 w-6 items-center justify-center rounded-full",
            "border border-glass-border-bright bg-canvas-100 text-white/60",
            "transition-all hover:border-cyan-neon/50 hover:text-cyan-neon hover:shadow-glow-sm-cyan"
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
            {/* Mobile hamburger */}
            <button
              onClick={() => setMobileOpen(true)}
              className="flex h-9 w-9 items-center justify-center rounded-lg border border-glass-border bg-glass-100 text-white/50 transition-all hover:bg-glass-200 hover:text-white/80 md:hidden"
              aria-label="Open menu"
            >
              <Menu className="h-4 w-4" />
            </button>

            {/* Page title */}
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

          {/* Right controls */}
          <div className="flex items-center gap-2 md:gap-3">
            {/* Search trigger */}
            <button
              className={cn(
                "flex h-9 items-center gap-2 rounded-lg border border-glass-border px-3",
                "bg-glass-100 text-xs text-white/40 transition-all",
                "hover:border-cyan-neon/30 hover:bg-glass-200 hover:text-white/70"
              )}
            >
              <Search className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">Search</span>
              <kbd className="hidden rounded bg-glass-300 px-1.5 py-0.5 font-mono text-2xs text-white/30 sm:inline">
                ⌘K
              </kbd>
            </button>

            {/* Notification bell */}
            <button
              className={cn(
                "relative flex h-9 w-9 items-center justify-center rounded-lg",
                "border border-glass-border bg-glass-100 text-white/50",
                "transition-all hover:border-cyan-neon/30 hover:bg-glass-200 hover:text-white/80"
              )}
              aria-label="Notifications"
            >
              <Bell className="h-4 w-4" />
              {/* Badge */}
              <span
                className="absolute -right-1 -top-1 flex h-4 w-4 items-center justify-center rounded-full text-2xs font-bold text-white"
                style={{
                  background: "#FF3B5C",
                  boxShadow: "0 0 8px rgba(255,59,92,0.6)",
                }}
              >
                4
              </span>
            </button>
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
