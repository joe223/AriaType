import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SettingsTransferMenu } from "../SettingsTransferMenu";

const {
  confirmMock,
  exportSettingsMock,
  importSettingsMock,
  loggerErrorMock,
  relaunchMock,
  selectSettingsImportMock,
  showErrorToastMock,
  showToastMock,
} = vi.hoisted(() => ({
  confirmMock: vi.fn(),
  exportSettingsMock: vi.fn(),
  importSettingsMock: vi.fn(),
  loggerErrorMock: vi.fn(),
  relaunchMock: vi.fn(),
  selectSettingsImportMock: vi.fn(),
  showErrorToastMock: vi.fn(),
  showToastMock: vi.fn(),
}));

vi.mock("@/components/ui/confirm", () => ({
  useConfirm: () => confirmMock,
}));

vi.mock("@/lib/tauri", () => ({
  settingsCommands: {
    exportSettings: exportSettingsMock,
    importSettings: importSettingsMock,
    selectSettingsImport: selectSettingsImportMock,
  },
}));

vi.mock("@/lib/toast", () => ({
  showErrorToast: showErrorToastMock,
  showToast: showToastMock,
}));

vi.mock("@/lib/logger", () => ({
  logger: {
    error: loggerErrorMock,
  },
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: relaunchMock,
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((promiseResolve) => {
    resolve = promiseResolve;
  });
  return { promise, resolve };
}

function chooseImport() {
  fireEvent.click(
    screen.getByRole("button", { name: "settingsTransfer.menu" }),
  );
  fireEvent.click(
    screen.getByRole("menuitem", { name: "settingsTransfer.import" }),
  );
}

describe("SettingsTransferMenu", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    confirmMock.mockResolvedValue(true);
    exportSettingsMock.mockResolvedValue({
      status: "saved",
      file_path: "/Users/test/Downloads/ariatype-settings.json",
    });
    importSettingsMock.mockResolvedValue({
      restart_required: true,
      restart_strategy: "automatic",
    });
    relaunchMock.mockResolvedValue(undefined);
    selectSettingsImportMock.mockResolvedValue({
      status: "selected",
      document: "{\"format\":\"ariatype-settings\"}",
    });
  });

  it("shows the sensitive-backup warning in the transfer menu", () => {
    render(<SettingsTransferMenu />);

    fireEvent.click(
      screen.getByRole("button", { name: "settingsTransfer.menu" }),
    );

    expect(
      screen.getByText("settingsTransfer.secretNote"),
    ).toBeInTheDocument();
  });

  it("shows the backend-provided Downloads export path", async () => {
    const savedPath = "/Users/test/Downloads/ariatype-settings.json";

    render(<SettingsTransferMenu />);

    fireEvent.click(
      screen.getByRole("button", { name: "settingsTransfer.menu" }),
    );
    fireEvent.click(
      screen.getByRole("menuitem", { name: "settingsTransfer.export" }),
    );

    await waitFor(() => {
      expect(exportSettingsMock).toHaveBeenCalledOnce();
      expect(showToastMock).toHaveBeenCalledWith(
        "settingsTransfer.exported",
        {
          description: savedPath,
          duration: 6000,
        },
      );
    });
  });

  it("shows an error when the backend cannot export to the selected path", async () => {
    exportSettingsMock.mockRejectedValueOnce("write failed");

    render(<SettingsTransferMenu />);

    fireEvent.click(
      screen.getByRole("button", { name: "settingsTransfer.menu" }),
    );
    fireEvent.click(
      screen.getByRole("menuitem", { name: "settingsTransfer.export" }),
    );

    await waitFor(() => {
      expect(showErrorToastMock).toHaveBeenCalledWith(
        "settingsTransfer.exportError",
      );
    });
    expect(showToastMock).not.toHaveBeenCalled();
  });

  it("uses backend-native selection instead of a hidden file input", async () => {
    selectSettingsImportMock.mockResolvedValueOnce({ status: "cancelled" });
    const { container } = render(<SettingsTransferMenu />);
    const menuButton = screen.getByRole("button", {
      name: "settingsTransfer.menu",
    });

    expect(container.querySelector('input[type="file"]')).toBeNull();
    chooseImport();

    await waitFor(() => expect(selectSettingsImportMock).toHaveBeenCalledOnce());
    await waitFor(() => expect(menuButton).toBeEnabled());
  });

  it("stays silent when native import selection is cancelled", async () => {
    const selection = deferred<{ status: "cancelled" }>();
    selectSettingsImportMock.mockReturnValueOnce(selection.promise);

    render(<SettingsTransferMenu />);

    const menuButton = screen.getByRole("button", {
      name: "settingsTransfer.menu",
    });
    chooseImport();

    await waitFor(() => expect(menuButton).toBeDisabled());
    await act(async () => {
      selection.resolve({ status: "cancelled" });
      await selection.promise;
    });
    await waitFor(() => expect(menuButton).toBeEnabled());
    expect(confirmMock).not.toHaveBeenCalled();
    expect(importSettingsMock).not.toHaveBeenCalled();
    expect(relaunchMock).not.toHaveBeenCalled();
    expect(showToastMock).not.toHaveBeenCalled();
    expect(showErrorToastMock).not.toHaveBeenCalled();
  });

  it("confirms before importing the selected document and relaunches", async () => {
    const document = "{\"format\":\"ariatype-settings\"}";
    const confirmation = deferred<boolean>();
    selectSettingsImportMock.mockResolvedValueOnce({
      status: "selected",
      document,
    });
    confirmMock.mockReturnValueOnce(confirmation.promise);

    render(<SettingsTransferMenu />);
    const menuButton = screen.getByRole("button", {
      name: "settingsTransfer.menu",
    });
    chooseImport();

    await waitFor(() => {
      expect(confirmMock).toHaveBeenCalledWith({
        title: "settingsTransfer.importConfirmTitle",
        description: "settingsTransfer.importConfirmDescription",
        confirmText: "settingsTransfer.importConfirmAction",
        cancelText: "common.cancel",
      });
    });
    expect(importSettingsMock).not.toHaveBeenCalled();

    await act(async () => {
      confirmation.resolve(true);
      await confirmation.promise;
    });
    await waitFor(() => {
      expect(importSettingsMock).toHaveBeenCalledWith(document);
      expect(showToastMock).toHaveBeenCalledWith("settingsTransfer.imported");
      expect(relaunchMock).toHaveBeenCalledOnce();
      expect(menuButton).toBeEnabled();
    });
  });

  it("requires a manual restart in development without relaunching", async () => {
    const document = "{\"format\":\"ariatype-settings\"}";
    importSettingsMock.mockResolvedValueOnce({
      restart_required: true,
      restart_strategy: "manual",
    });
    selectSettingsImportMock.mockResolvedValueOnce({
      status: "selected",
      document,
    });

    render(<SettingsTransferMenu />);
    const menuButton = screen.getByRole("button", {
      name: "settingsTransfer.menu",
    });
    chooseImport();

    await waitFor(() => {
      expect(importSettingsMock).toHaveBeenCalledWith(document);
      expect(showToastMock).toHaveBeenCalledWith(
        "settingsTransfer.imported",
        {
          description: "settingsTransfer.developmentRestartRequired",
          duration: 8000,
        },
      );
      expect(menuButton).toBeEnabled();
    });
    expect(relaunchMock).not.toHaveBeenCalled();
    expect(showErrorToastMock).not.toHaveBeenCalled();
  });

  it("shows only the restart error when import succeeds but relaunch fails", async () => {
    const relaunchError = "sensitive-relaunch-error";
    relaunchMock.mockRejectedValue(relaunchError);
    const document = "{\"format\":\"ariatype-settings\"}";
    selectSettingsImportMock.mockResolvedValueOnce({
      status: "selected",
      document,
    });

    render(<SettingsTransferMenu />);
    chooseImport();

    await waitFor(() => {
      expect(importSettingsMock).toHaveBeenCalledWith(document);
      expect(relaunchMock).toHaveBeenCalledOnce();
      expect(showErrorToastMock).toHaveBeenCalledWith(
        "settingsTransfer.restartError",
      );
    });
    expect(showToastMock).toHaveBeenCalledWith("settingsTransfer.imported");
    expect(showErrorToastMock).toHaveBeenCalledOnce();
    expect(showErrorToastMock).not.toHaveBeenCalledWith(
      "settingsTransfer.importError",
    );
    expect(JSON.stringify(loggerErrorMock.mock.calls)).not.toContain(
      relaunchError,
    );
  });

  it("does not import when confirmation is declined", async () => {
    const confirmation = deferred<boolean>();
    confirmMock.mockReturnValueOnce(confirmation.promise);

    render(<SettingsTransferMenu />);

    const menuButton = screen.getByRole("button", {
      name: "settingsTransfer.menu",
    });
    chooseImport();
    await waitFor(() => expect(confirmMock).toHaveBeenCalledOnce());

    await act(async () => {
      confirmation.resolve(false);
      await confirmation.promise;
    });
    await waitFor(() => expect(menuButton).toBeEnabled());
    expect(importSettingsMock).not.toHaveBeenCalled();
    expect(relaunchMock).not.toHaveBeenCalled();
    expect(showToastMock).not.toHaveBeenCalled();
    expect(showErrorToastMock).not.toHaveBeenCalled();
  });

  it("shows an import error when native selection or bounded reading fails", async () => {
    const sensitiveError = "failed near /Users/private/settings.json";
    selectSettingsImportMock.mockRejectedValueOnce(sensitiveError);

    render(<SettingsTransferMenu />);
    chooseImport();

    await waitFor(() => {
      expect(showErrorToastMock).toHaveBeenCalledWith(
        "settingsTransfer.importError",
      );
    });
    expect(confirmMock).not.toHaveBeenCalled();
    expect(importSettingsMock).not.toHaveBeenCalled();
    expect(relaunchMock).not.toHaveBeenCalled();
    expect(showToastMock).not.toHaveBeenCalled();
    expect(JSON.stringify(loggerErrorMock.mock.calls)).not.toContain(
      sensitiveError,
    );
  });

  it("does not leak the imported document when the backend rejects it", async () => {
    const document = "super-secret-settings-document";
    importSettingsMock.mockRejectedValue(document);
    selectSettingsImportMock.mockResolvedValueOnce({
      status: "selected",
      document,
    });

    render(<SettingsTransferMenu />);
    chooseImport();

    await waitFor(() => {
      expect(showErrorToastMock).toHaveBeenCalledWith(
        "settingsTransfer.importError",
      );
    });
    expect(JSON.stringify(loggerErrorMock.mock.calls)).not.toContain(document);
  });
});
