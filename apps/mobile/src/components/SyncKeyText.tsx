// A sync key as it is written down: grouped, monospaced, selectable.
export function SyncKeyText({ value }: { value: string }) {
  return (
    <div
      className="mono rounded-10 bg-inset px-3 py-2 text-[16px] break-all text-fg select-all hairline"
      data-testid="sync-key">
      {value}
    </div>
  );
}
