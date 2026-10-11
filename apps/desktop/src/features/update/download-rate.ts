import type { Backend, TFunction } from "@lockra/shared";
import { useEffect, useState } from "react";

/** One reading of a download: when (ms, monotonic) and how many bytes had arrived. */
export interface RateSample {
  at: number;
  received: number;
}

/** How far back the speed looks: long enough to smooth a bursty download, short enough to follow
 *  a slowdown. */
export const RATE_WINDOW_MS = 3000;
/** Less than this much history says nothing about the speed yet. */
export const RATE_MIN_SPAN_MS = 500;

/** Bytes per second over the samples, or nothing until they span `RATE_MIN_SPAN_MS`. */
export function rateFrom(samples: readonly RateSample[]): number | undefined {
  const first = samples[0];
  const last = samples.at(-1);
  if (first === undefined || last === undefined) return undefined;
  const span = last.at - first.at;
  if (span < RATE_MIN_SPAN_MS || last.received < first.received) return undefined;
  return ((last.received - first.received) * 1000) / span;
}

/** The remaining time, worded, or nothing when it cannot be told. */
export function etaText(
  received: number,
  total: number | null,
  rate: number | undefined,
  t: TFunction,
): string | undefined {
  if (total === null || rate === undefined || rate <= 0 || received >= total) return undefined;
  const seconds = Math.ceil((total - received) / rate);
  if (seconds < 60) return t("update.eta.seconds", { s: seconds });
  return t("update.eta.minutes", { m: Math.floor(seconds / 60), s: seconds % 60 });
}

const monotonic = () => performance.now();

/** The speed of the download, from the states of the last `RATE_WINDOW_MS` (their arrival times);
 *  forgotten as soon as the updater is not downloading (ported from Voltip). The dialog stays
 *  mounted while it is closed, so the readings start with the download. */
export function useDownloadRate(
  backend: Pick<Backend, "on">,
  now: () => number = monotonic,
): number | undefined {
  const [rate, setRate] = useState<number | undefined>(undefined);
  useEffect(() => {
    let samples: RateSample[] = [];
    return backend.on((event) => {
      if (event.type !== "state") return;
      const status = event.state.update.status;
      if (status.state !== "downloading") {
        if (samples.length > 0) {
          samples = [];
          setRate(undefined);
        }
        return;
      }
      const at = now();
      samples = [
        ...samples.filter((sample) => at - sample.at <= RATE_WINDOW_MS),
        { at, received: status.received },
      ];
      setRate(rateFrom(samples));
    });
  }, [backend, now]);
  return rate;
}
