import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge Tailwind classes safely — handles conflicts and conditional classes */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
