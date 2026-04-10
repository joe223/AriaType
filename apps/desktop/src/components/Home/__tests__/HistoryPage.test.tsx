import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { HistoryPage } from "@/components/Home/HistoryPage";
import i18n from "@/i18n";
import { historyCommands, textCommands, windowCommands } from "@/lib/tauri";

vi.mock("@/lib/logger", () => ({
  logger: {
    error: vi.fn(),
  },
}));

vi.mock("@/lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("@/lib/tauri")>("@/lib/tauri");
  return {
    ...actual,
    historyCommands: {
      ...actual.historyCommands,
      getHistory: vi.fn(),
    },
    textCommands: {
      ...actual.textCommands,
      copyToClipboard: vi.fn(),
    },
    windowCommands: {
      ...actual.windowCommands,
      showToast: vi.fn(),
    },
  };
});

describe("HistoryPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    void i18n.changeLanguage("en");
  });

  it("copies full history text from an entry action", async () => {
    vi.mocked(historyCommands.getHistory).mockResolvedValue([
      {
        id: "entry-1",
        created_at: Date.now() - 20_000,
        raw_text: "raw text",
        final_text: "First capture text",
        stt_engine: "local-whisper",
        stt_model: "whisper-base",
        language: "en",
        audio_duration_ms: 2300,
        stt_duration_ms: 600,
        polish_duration_ms: null,
        total_duration_ms: 600,
        polish_applied: false,
        polish_engine: null,
        is_cloud: false,
      },
    ]);
    vi.mocked(textCommands.copyToClipboard).mockResolvedValue(undefined);
    vi.mocked(windowCommands.showToast).mockResolvedValue(undefined);

    render(<HistoryPage />);

    await waitFor(() => {
      expect(screen.getByText("First capture text")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Copy" }));

    await waitFor(() => {
      expect(textCommands.copyToClipboard).toHaveBeenCalledWith("First capture text");
    });
    await waitFor(() => {
      expect(windowCommands.showToast).toHaveBeenCalledWith("Copied!");
    });
  });
});
