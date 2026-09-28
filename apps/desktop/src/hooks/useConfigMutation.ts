import { useCallback, useRef, useState } from "react";

type Mutation = "switch" | "restore";

/** The ref acquires synchronously, including before React renders disabled controls. */
export function useConfigMutation() {
  const owner = useRef<Mutation | null>(null);
  const [operation, setOperation] = useState<Mutation | null>(null);
  const [revision, setRevision] = useState(0);
  const invalidate = useCallback(() => setRevision((current) => current + 1), []);
  const begin = useCallback((kind: Mutation) => {
    if (owner.current !== null) return false;
    owner.current = kind;
    setOperation(kind);
    invalidate();
    return true;
  }, [invalidate]);
  const end = useCallback(() => {
    owner.current = null;
    setOperation(null);
    invalidate();
  }, [invalidate]);
  return { operation, revision, begin, end, invalidate };
}
