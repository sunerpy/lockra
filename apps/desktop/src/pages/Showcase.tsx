// Development only (`#showcase`, never in a release bundle): every primitive in every state, to be
// checked in the four themes before pages are built on them (DESIGN.md §3). Keys 1–4 switch the
// theme, so a headless run can screenshot all four without clicking.
import { type CodeView, type EntryView, THEME_IDS, type ThemeId } from "@lockra/shared";
import {
  Badge,
  Banner,
  Button,
  Card,
  Chip,
  CountdownRing,
  DropZone,
  EmptyState,
  EntryRow,
  Eyebrow,
  Icon,
  IconButton,
  Input,
  Keycaps,
  Lamp,
  Logo,
  OtpCode,
  Panel,
  PasswordField,
  Progress,
  QrView,
  Segmented,
  Select,
  StatusRow,
  StepList,
  ThemeTile,
  Toggle,
  applyTheme,
  useClock,
  useT,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { QR_SAMPLE } from "./showcase-samples";

function sampleEntry(
  id: string,
  issuer: string,
  account: string,
  patch: Partial<EntryView> = {},
): EntryView {
  return {
    id,
    issuer,
    account,
    kind: { type: "totp", period: 30 },
    algorithm: "sha1",
    digits: 6,
    group: null,
    favorite: false,
    origin: "uri",
    created_at_ms: 0,
    updated_at_ms: 0,
    last_used_at_ms: null,
    export: { google: null, microsoft: null },
    ...patch,
  };
}

/** `#showcase-freeze`: a fixed moment 12 s into a window and no animation, for screenshots. */
const FROZEN_NOW = 1_790_000_012_000;
const FREEZE_STYLE =
  "[data-freeze] *, [data-freeze] *::before, [data-freeze] *::after { animation: none !important; transition: none !important; }";

export default function Showcase() {
  const t = useT();
  const live = useClock();
  const frozen = location.hash.includes("freeze");
  const now = frozen ? FROZEN_NOW : live;
  const [theme, setTheme] = useState<ThemeId>("light");
  const [toggle, setToggle] = useState(true);
  const [segment, setSegment] = useState<"name" | "added" | "recent">("name");
  const [password, setPassword] = useState("correct horse");
  useEffect(() => applyTheme(theme), [theme]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const index = Number(event.key) - 1;
      const next = THEME_IDS[index];
      if (next) setTheme(next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const window30 = Math.floor(now / 30_000) * 30_000;
  const code = (id: string, offsetMs = 0): CodeView => ({
    entry_id: id,
    code: "492039",
    next_code: "114415",
    valid_from_ms: window30 - offsetMs,
    valid_until_ms: window30 + 30_000 - offsetMs,
  });
  const late: CodeView = {
    entry_id: "w",
    code: "731906",
    next_code: "530214",
    valid_from_ms: now - 26_000,
    valid_until_ms: now + 4_000,
  };
  return (
    <div
      data-testid="showcase"
      data-freeze={frozen || undefined}
      className="grid h-full grid-rows-[auto_minmax(0,1fr)] bg-canvas text-fg">
      {frozen && <style>{FREEZE_STYLE}</style>}
      <header className="flex h-10 items-center gap-3 border-b border-border bg-surface px-4">
        <Logo size={20} />
        <span className="text-[14px] font-semibold">{t("showcase.title")}</span>
        <span className="mono text-[11px] text-fg-subtle">theme={theme} · 1–4</span>
      </header>
      <main className="min-h-0 overflow-y-auto">
        <div className="mx-auto flex max-w-[1040px] flex-col gap-4 px-6 py-5">
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4">
            <Panel eyebrow="BUTTONS" title="Button / IconButton / Keycaps">
              <div className="flex flex-wrap items-center gap-2">
                <Button variant="primary" icon="plus" keys="Ctrl N">
                  添加
                </Button>
                <Button variant="outline">导入</Button>
                <Button variant="ghost">取消</Button>
                <Button variant="danger" icon="trash">
                  删除
                </Button>
                <Button variant="primary" loading>
                  解锁
                </Button>
                <Button variant="outline" disabled>
                  不可用
                </Button>
                <Button variant="text">查看全部</Button>
                <IconButton icon="copy" label="复制" />
                <IconButton icon="trash" label="删除" tone="danger" bordered size={28} />
                <Keycaps keys="Ctrl K" />
              </div>
            </Panel>
            <Panel eyebrow="STATUS" title="Badge / Chip / Lamp / Progress">
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone="ok">已解锁</Badge>
                <Badge tone="accent">新增</Badge>
                <Badge tone="warn">同名不同密钥</Badge>
                <Badge tone="danger">不支持</Badge>
                <Badge tone="info">Google</Badge>
                <Chip>工作</Chip>
                <Lamp tone="ok" label="ok" />
                <Lamp tone="danger" />
                <Lamp tone="idle" pulse />
              </div>
              <div className="mt-3 flex flex-col gap-2">
                <Progress value={0.62} />
                <Progress indeterminate />
                <Progress value={0.5} segments={4} tone="ok" />
              </div>
            </Panel>
          </div>
          <Panel eyebrow="CODES" title="EntryRow · OtpCode · CountdownRing">
            <div className="flex flex-col">
              <EntryRow
                entry={sampleEntry("a", "GitHub", "octocat", { favorite: true })}
                code={code("a")}
                nowMs={now}
                onCopy={() => undefined}
              />
              <EntryRow
                entry={sampleEntry("w", "Microsoft", "alex@outlook.com", { digits: 8 })}
                code={{ ...late, code: "73190634", next_code: "53021477" }}
                nowMs={now}
                onCopy={() => undefined}
              />
              <EntryRow
                entry={sampleEntry("m", "AWS", "root@acme-corp")}
                code={code("m", 9_000)}
                nowMs={now}
                masked
                onCopy={() => undefined}
              />
              <EntryRow
                entry={sampleEntry("h", "Bank", "6222 •••• 1234", {
                  kind: { type: "hotp", counter: 12 },
                })}
                code={{
                  entry_id: "h",
                  code: "287082",
                  next_code: null,
                  valid_from_ms: null,
                  valid_until_ms: null,
                }}
                nowMs={now}
                onCopy={() => undefined}
                onNext={() => undefined}
              />
              <EntryRow
                entry={sampleEntry(
                  "l",
                  "A very long issuer name that has to be cut somewhere ".repeat(3),
                  "an.account.name.without.any.spaces.at.all@an-unusually-long-domain.example",
                )}
                nowMs={now}
                onCopy={() => undefined}
              />
            </div>
            <div className="mt-3 flex items-center gap-4">
              <OtpCode code="492039" size="lg" />
              <OtpCode code="12345678" tone="warning" />
              <OtpCode code="492039" masked size="sm" />
              <CountdownRing
                validFromMs={window30}
                validUntilMs={window30 + 30_000}
                nowMs={now}
                size={28}
              />
              <CountdownRing
                validFromMs={now - 27_000}
                validUntilMs={now + 3_000}
                nowMs={now}
                size={28}
                still
              />
            </div>
          </Panel>
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4">
            <Panel eyebrow="FORMS" title="Input / PasswordField / Select / Segmented / Toggle">
              <div className="flex flex-col gap-3">
                <Input label="服务名称" placeholder="例如：GitHub" icon="search" />
                <Input
                  label="密钥"
                  mono
                  defaultValue="JBSW Y3DP"
                  error="密钥无效：应为 Base32 字母和数字"
                />
                <Input label="不可用" disabled defaultValue="disabled" />
                <PasswordField
                  label="主密码"
                  value={password}
                  onChange={setPassword}
                  strength
                  help="至少 8 个字符"
                />
                <Select
                  label="自动锁定"
                  value="5"
                  onChange={() => undefined}
                  options={[
                    { value: "5", label: "5 分钟" },
                    { value: "10", label: "10 分钟" },
                  ]}
                />
                <Segmented
                  value={segment}
                  onChange={setSegment}
                  options={[
                    { value: "name", label: "按名称" },
                    { value: "added", label: "最近添加" },
                    { value: "recent", label: "最近使用" },
                  ]}
                />
                <Toggle checked={toggle} onChange={setToggle} label="在本机记住" />
                <Toggle checked={false} onChange={() => undefined} label="不可用" disabled />
              </div>
            </Panel>
            <div className="flex flex-col gap-4">
              <Panel eyebrow="IMPORT" title="DropZone / StepList">
                <div className="flex flex-col gap-3">
                  <StepList
                    steps={[
                      "在手机上打开 Google 身份验证器，点 ⋮ → 转移账号 → 导出账号。",
                      "用另一台设备拍下二维码。",
                      "把图片拖到这里。",
                    ]}
                  />
                  <DropZone
                    title="选择图片…"
                    hint="也可以把文件拖到窗口里"
                    onActivate={() => undefined}
                  />
                  <DropZone title="松开以导入" active icon="image" onActivate={() => undefined} />
                </div>
              </Panel>
              <Panel eyebrow="EXPORT" title="QrView">
                <QrView svg={QR_SAMPLE} size={160} footer="第 1 / 2 张" />
              </Panel>
            </div>
          </div>
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4">
            <div className="flex flex-col gap-3">
              <Banner tone="info" title="Google 导出：已收到 1/2 张">
                还缺第 2 张二维码
              </Banner>
              <Banner tone="danger" title="自动备份失败">
                备份文件夹无法写入
              </Banner>
              <Banner tone="warn">Linux 无法阻止截屏或录屏，请注意屏幕共享。</Banner>
              <Card>
                <Eyebrow right="12 s">CARD</Eyebrow>
                <StatusRow label="自动锁定" help="这段时间没有操作就锁定保险库">
                  <span className="text-[12px] text-fg-muted">5 分钟</span>
                </StatusRow>
              </Card>
            </div>
            <div className="flex flex-col gap-3">
              <EmptyState
                title="还没有账号"
                icon="key"
                actions={<Button variant="primary">导入</Button>}>
                从 Google 身份验证器或 Microsoft Authenticator 导入，或者手动添加。
              </EmptyState>
              <div className="flex flex-wrap gap-4">
                {THEME_IDS.map((id) => (
                  <ThemeTile key={id} theme={id} selected={id === theme} onSelect={setTheme} />
                ))}
              </div>
              <div className="flex gap-2 text-fg-muted">
                {(
                  [
                    "lock",
                    "unlock",
                    "key",
                    "archive",
                    "scan",
                    "clipboard",
                    "fileText",
                    "eyeOff",
                    "download",
                    "upload",
                  ] as const
                ).map((name) => (
                  <Icon key={name} name={name} />
                ))}
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
