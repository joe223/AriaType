import { beforeEach, describe, expect, it, vi } from "vitest";

import { settingsCommands } from "../tauri";

const { debugMock, errorMock, invokeMock } = vi.hoisted(() => ({
  debugMock: vi.fn(),
  errorMock: vi.fn(),
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  Channel: class Channel {},
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("../logger", () => ({
  logger: {
    debug: debugMock,
    error: errorMock,
  },
}));

describe("settings transfer IPC", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockResolvedValue({ restart_required: false });
  });

  it("redacts imported settings documents from IPC logs", async () => {
    const document = "super-secret-settings-document";

    await settingsCommands.importSettings(document);

    expect(invokeMock).toHaveBeenCalledWith("import_settings", { document });
    expect(debugMock).toHaveBeenCalledWith("ipc_request", {
      command: "import_settings",
      args: "[redacted]",
    });
    expect(JSON.stringify(debugMock.mock.calls)).not.toContain(document);
  });

  it("does not leak an imported document when a sensitive IPC call fails", async () => {
    const document = "super-secret-settings-document";
    invokeMock.mockRejectedValue(document);

    await expect(settingsCommands.importSettings(document)).rejects.toBe(
      document,
    );

    expect(JSON.stringify(errorMock.mock.calls)).not.toContain(document);
  });

  it("does not log a document returned by native import selection", async () => {
    const document = "super-secret-selected-settings-document";
    invokeMock.mockResolvedValueOnce({ status: "selected", document });

    await expect(settingsCommands.selectSettingsImport()).resolves.toEqual({
      status: "selected",
      document,
    });

    expect(invokeMock).toHaveBeenCalledWith(
      "select_settings_import",
      undefined,
    );
    expect(JSON.stringify(debugMock.mock.calls)).not.toContain(document);
    expect(JSON.stringify(errorMock.mock.calls)).not.toContain(document);
  });

  it("redacts native import selection errors", async () => {
    const sensitiveError = "failed near /Users/private/settings.json";
    invokeMock.mockRejectedValueOnce(sensitiveError);

    await expect(settingsCommands.selectSettingsImport()).rejects.toBe(
      sensitiveError,
    );

    expect(JSON.stringify(errorMock.mock.calls)).not.toContain(sensitiveError);
  });
});
