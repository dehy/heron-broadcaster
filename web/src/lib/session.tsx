// SPDX-License-Identifier: AGPL-3.0-or-later

// The signed-in user, and mutations that report their errors.

import { type QueryKey, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createContext, useContext } from "react";
import { type Role, type User, can, errorMessage, fetchMe } from "../api";
import { useToast } from "../components/ui";

export const ME_KEY = ["me"] as const;

export function useMeQuery() {
  return useQuery({ queryKey: ME_KEY, queryFn: fetchMe, staleTime: 60_000, retry: 1 });
}

export const UserContext = createContext<User | null>(null);

/** The signed-in user (inside the signed-in app). */
export function useUser(): User {
  const user = useContext(UserContext);
  if (!user) {
    throw new Error("useUser outside the signed-in app");
  }
  return user;
}

/** Whether the signed-in user's role allows what `needed` can do. */
export function useCan(needed: Role): boolean {
  return can(useContext(UserContext)?.role, needed);
}

/**
 * A mutation that shows a toast on success (when given a message) and on
 * failure, and refreshes the queries under `invalidate`.
 */
export function useAction<V, R>(
  action: (variables: V) => Promise<R>,
  options: {
    success?: string | ((result: R) => string);
    invalidate?: QueryKey[];
    onSuccess?: (result: R, variables: V) => void;
  } = {},
) {
  const client = useQueryClient();
  const toast = useToast();
  return useMutation({
    mutationFn: action,
    onSuccess: async (result, variables) => {
      for (const key of options.invalidate ?? []) {
        await client.invalidateQueries({ queryKey: key });
      }
      if (options.success) {
        toast(typeof options.success === "string" ? options.success : options.success(result));
      }
      options.onSuccess?.(result, variables);
    },
    onError: (error) => toast(errorMessage(error), true),
  });
}
