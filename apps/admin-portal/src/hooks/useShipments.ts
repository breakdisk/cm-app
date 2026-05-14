"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import { ShipmentDto, listShipments } from "@/lib/api/shipments";

const PER_PAGE = 25;

export interface UseShipmentsReturn {
  shipments: ShipmentDto[];
  total: number;
  page: number;
  totalPages: number;
  loading: boolean;
  error: string | null;
  search: string;
  statusFilter: string;
  setSearch: (q: string) => void;
  setStatusFilter: (s: string) => void;
  setPage: (p: number) => void;
  refresh: () => void;
}

export function useShipments(): UseShipmentsReturn {
  const [shipments,    setShipments]    = useState<ShipmentDto[]>([]);
  const [total,        setTotal]        = useState(0);
  const [page,         setPageRaw]      = useState(1);
  const [loading,      setLoading]      = useState(false);
  const [error,        setError]        = useState<string | null>(null);
  const [search,       setSearchRaw]    = useState("");
  const [debouncedQ,   setDebouncedQ]   = useState("");
  const [statusFilter, setStatusRaw]    = useState("all");

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear any pending debounce on unmount to avoid state updates after unmount
  useEffect(() => () => { if (debounceRef.current) clearTimeout(debounceRef.current); }, []);

  const setSearch = useCallback((q: string) => {
    setSearchRaw(q);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      setDebouncedQ(q.trim());
      setPageRaw(1);
    }, 300);
  }, []);

  const setStatusFilter = useCallback((s: string) => {
    setStatusRaw(s);
    setPageRaw(1);
  }, []);

  const setPage = useCallback((p: number) => {
    setPageRaw(p);
  }, []);

  const fetch = useCallback(async (opts: {
    status: string;
    q: string;
    page: number;
  }) => {
    setLoading(true);
    setError(null);
    try {
      const result = await listShipments({
        status:   opts.status === "all" ? undefined : opts.status,
        q:        opts.q || undefined,
        page:     opts.page,
        per_page: PER_PAGE,
      });
      setShipments(result.shipments);
      setTotal(result.total);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load shipments");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetch({ status: statusFilter, q: debouncedQ, page });
  }, [fetch, statusFilter, debouncedQ, page]);

  const refresh = useCallback(() => {
    fetch({ status: statusFilter, q: debouncedQ, page });
  }, [fetch, statusFilter, debouncedQ, page]);

  const totalPages = Math.max(1, Math.ceil(total / PER_PAGE));

  return {
    shipments,
    total,
    page,
    totalPages,
    loading,
    error,
    search,
    statusFilter,
    setSearch,
    setStatusFilter,
    setPage,
    refresh,
  };
}
