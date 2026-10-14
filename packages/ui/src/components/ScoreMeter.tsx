import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";

const STRENGTH = [
  "generator.strength.s0",
  "generator.strength.s1",
  "generator.strength.s2",
  "generator.strength.s3",
  "generator.strength.s4",
] as const;

/** A password's strength from zxcvbn's score (`password_score`), four bars and its name, and the
 *  generator's bits when it made the password (boards 08, 09, 28). */
export function ScoreMeter({ score, bits }: { score: number | undefined; bits?: number }) {
  const t = useT();
  const label =
    score === undefined
      ? ""
      : t(STRENGTH[Math.min(4, Math.max(0, score))] ?? "generator.strength.s0");
  return (
    <div className="flex items-center gap-2" data-testid="generator-strength">
      <span className="flex gap-[3px]" aria-hidden>
        {[0, 1, 2, 3].map((i) => (
          <span
            key={i}
            className={cx(
              "h-[3.5px] w-9 rounded-pill",
              score !== undefined && i < Math.max(1, score)
                ? score >= 3
                  ? "bg-accent"
                  : "bg-warning"
                : "bg-track",
            )}
          />
        ))}
      </span>
      <span className="text-[12px] text-fg-muted">
        {score === undefined
          ? ""
          : bits === undefined
            ? label
            : t("generator.strength.bits", { label, n: bits })}
      </span>
    </div>
  );
}
