"use client";
import { useRouter } from "next/navigation";
import { useEffect } from "react";

export default function CrmPage() {
  const router = useRouter();
  useEffect(() => { router.replace("/crm/automations"); }, [router]);
  return null;
}
