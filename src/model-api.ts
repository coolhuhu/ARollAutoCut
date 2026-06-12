import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type ModelState = "missing" | "invalid" | "ready";

export interface ModelStatus {
  state: ModelState;
  directory: string;
  issues: string[];
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function getModelStatus(): Promise<ModelStatus> {
  if (!isTauriRuntime()) {
    return {
      state: "missing",
      directory: "Tauri App Data/models",
      issues: ["请在 Tauri App 中配置 SenseVoice 模型"],
    };
  }
  return invoke<ModelStatus>("get_model_status");
}

export async function chooseModelDirectory(): Promise<ModelStatus | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const directory = await open({
    directory: true,
    multiple: false,
    title: "选择 SenseVoice 模型目录",
  });
  if (typeof directory !== "string") {
    return null;
  }

  return invoke<ModelStatus>("select_model_directory", { directory });
}
