import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle2, Loader2, XCircle } from "lucide-react";
import type { ModelConfigView } from "@/types/note";

type TestState = { type: "idle" | "loading" | "success" | "error"; message: string };

export default function ModelSettingsDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [apiUrl, setApiUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [hasApiKey, setHasApiKey] = useState(false);
  const [testState, setTestState] = useState<TestState>({ type: "idle", message: "" });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setApiKey("");
    setTestState({ type: "idle", message: "" });
    invoke<ModelConfigView>("get_model_config")
      .then((config) => {
        setApiUrl(config.api_url || "https://api.openai.com/v1");
        setModel(config.model || "");
        setHasApiKey(config.has_api_key);
      })
      .catch((error) => {
        setTestState({ type: "error", message: String(error) });
      });
  }, [open]);

  if (!open) return null;

  const testConfig = async () => {
    setTestState({ type: "loading", message: "正在测试..." });
    try {
      await invoke("test_model_config", { apiUrl, apiKey, model });
      setTestState({ type: "success", message: "测试成功，模型可用。" });
      setHasApiKey(true);
    } catch (error) {
      setTestState({ type: "error", message: String(error) });
    }
  };

  const saveConfig = async () => {
    setSaving(true);
    setTestState({ type: "idle", message: "" });
    try {
      await invoke("save_model_config", { apiUrl, apiKey, model });
      onClose();
    } catch (error) {
      setTestState({ type: "error", message: String(error) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/30">
      <div className="w-[520px] max-w-[calc(100vw-32px)] rounded-md border border-[#d8dee4] bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-[#d8dee4] px-4 py-3">
          <div>
            <h2 className="text-sm font-semibold text-[#24292f]">模型设置</h2>
            <p className="mt-1 text-xs text-[#57606a]">配置 OpenAI 兼容接口后可使用 HTML/Markdown 转换。</p>
          </div>
          <button className="rounded px-2 py-1 text-sm text-[#57606a] hover:bg-[#eaeef2]" onClick={onClose}>
            关闭
          </button>
        </div>

        <div className="space-y-4 px-4 py-4">
          <label className="block">
            <span className="text-xs font-medium text-[#57606a]">API 请求地址</span>
            <input
              className="mt-1 w-full rounded border border-[#d8dee4] px-3 py-2 text-sm outline-none focus:border-[#0969da]"
              placeholder="https://api.openai.com/v1"
              value={apiUrl}
              onChange={(event) => setApiUrl(event.target.value)}
            />
          </label>
          <label className="block">
            <span className="text-xs font-medium text-[#57606a]">API 密钥</span>
            <input
              className="mt-1 w-full rounded border border-[#d8dee4] px-3 py-2 text-sm outline-none focus:border-[#0969da]"
              type="password"
              placeholder={hasApiKey ? "已保存密钥；留空表示不修改" : "请输入 API 密钥"}
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
            />
          </label>
          <label className="block">
            <span className="text-xs font-medium text-[#57606a]">模型名称</span>
            <input
              className="mt-1 w-full rounded border border-[#d8dee4] px-3 py-2 text-sm outline-none focus:border-[#0969da]"
              placeholder="gpt-4o-mini"
              value={model}
              onChange={(event) => setModel(event.target.value)}
            />
          </label>

          {testState.type !== "idle" && (
            <div
              className={`flex items-center gap-2 rounded border px-3 py-2 text-xs ${
                testState.type === "success"
                  ? "border-[#2da44e]/30 bg-[#dafbe1] text-[#116329]"
                  : testState.type === "error"
                    ? "border-red-200 bg-red-50 text-red-700"
                    : "border-[#d8dee4] bg-[#f6f8fa] text-[#57606a]"
              }`}
            >
              {testState.type === "loading" && <Loader2 size={14} className="animate-spin" />}
              {testState.type === "success" && <CheckCircle2 size={14} />}
              {testState.type === "error" && <XCircle size={14} />}
              <span className="break-all">{testState.message}</span>
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-[#d8dee4] px-4 py-3">
          <button className="rounded border border-[#d8dee4] px-3 py-1.5 text-sm hover:bg-[#f6f8fa]" onClick={testConfig}>
            测试
          </button>
          <button className="rounded border border-[#d8dee4] px-3 py-1.5 text-sm hover:bg-[#f6f8fa]" onClick={onClose}>
            取消
          </button>
          <button
            className="rounded bg-[#2da44e] px-3 py-1.5 text-sm text-white hover:bg-[#2c974b] disabled:opacity-60"
            disabled={saving}
            onClick={saveConfig}
          >
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
