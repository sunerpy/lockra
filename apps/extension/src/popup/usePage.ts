// The popup's (and the options page's) link to the background: its state, search results and the
// outcome of fills and copies, as the background sends them.
import { useCallback, useEffect, useRef, useState } from "react";
import type { ItemSummary } from "../protocol/schemas";
import type { PopupEvent, PopupRequest, PopupState } from "../shared/messages";
import type { PageClient } from "./client";

export interface SearchResult {
  text: string;
  items: ItemSummary[];
  error: string | null;
}

export type Results = Record<string, { action: "fill" | "copy"; outcome: string; part?: string }>;

export interface Page {
  state: PopupState | null;
  search: SearchResult | null;
  results: Results;
  send: (request: PopupRequest) => void;
}

export function usePage(client: PageClient): Page {
  const [state, setState] = useState<PopupState | null>(null);
  const [search, setSearch] = useState<SearchResult | null>(null);
  const [results, setResults] = useState<Results>({});
  const link = useRef<{ send: (request: PopupRequest) => void; close: () => void } | null>(null);

  useEffect(() => {
    const connection = client.connect((event: PopupEvent) => {
      if (event.kind === "state") setState(event.state);
      else if (event.kind === "search")
        setSearch({ text: event.text, items: event.items, error: event.error });
      else
        setResults((now) => ({
          ...now,
          [event.itemId]: {
            action: event.action,
            outcome: event.outcome,
            ...(event.part ? { part: event.part } : {}),
          },
        }));
    });
    link.current = connection;
    connection.send({ kind: "hello" });
    return () => {
      link.current = null;
      connection.close();
    };
  }, [client]);

  const send = useCallback((request: PopupRequest) => link.current?.send(request), []);
  return { state, search, results, send };
}
