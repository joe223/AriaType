import { useState } from "react";
import {
  autoUpdate,
  flip,
  FloatingFocusManager,
  FloatingPortal,
  offset,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useInteractions,
  useRole,
} from "@floating-ui/react";
import { DotsThree, DownloadSimple, UploadSimple } from "@phosphor-icons/react";
import { relaunch } from "@tauri-apps/plugin-process";
import { useTranslation } from "react-i18next";

import { useConfirm } from "@/components/ui/confirm";
import { logger } from "@/lib/logger";
import { settingsCommands } from "@/lib/tauri";
import { showErrorToast, showToast } from "@/lib/toast";

export function SettingsTransferMenu() {
  const { t } = useTranslation();
  const confirm = useConfirm();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);

  const { context, floatingStyles, refs } = useFloating({
    open,
    onOpenChange: setOpen,
    placement: "bottom-end",
    whileElementsMounted: autoUpdate,
    middleware: [offset(6), flip({ padding: 8 }), shift({ padding: 8 })],
  });

  const click = useClick(context);
  const dismiss = useDismiss(context);
  const role = useRole(context, { role: "menu" });
  const { getFloatingProps, getReferenceProps } = useInteractions([
    click,
    dismiss,
    role,
  ]);

  const handleExport = async () => {
    setOpen(false);
    setBusy(true);

    try {
      const result = await settingsCommands.exportSettings();
      showToast(t("settingsTransfer.exported"), {
        description: result.file_path,
        duration: 6000,
      });
    } catch (error) {
      logger.error("failed_to_export_settings", { error: String(error) });
      showErrorToast(t("settingsTransfer.exportError"));
    } finally {
      setBusy(false);
    }
  };

  const handleImport = async () => {
    setOpen(false);
    setBusy(true);

    try {
      const selection = await settingsCommands.selectSettingsImport();
      if (selection.status === "cancelled") {
        return;
      }

      const confirmed = await confirm({
        title: t("settingsTransfer.importConfirmTitle"),
        description: t("settingsTransfer.importConfirmDescription"),
        confirmText: t("settingsTransfer.importConfirmAction"),
        cancelText: t("common.cancel"),
      });

      if (!confirmed) {
        return;
      }

      const result = await settingsCommands.importSettings(selection.document);

      if (
        result.restart_required &&
        result.restart_strategy === "manual"
      ) {
        showToast(t("settingsTransfer.imported"), {
          description: t("settingsTransfer.developmentRestartRequired"),
          duration: 8000,
        });
        return;
      }

      showToast(t("settingsTransfer.imported"));

      if (result.restart_required) {
        try {
          await relaunch();
        } catch {
          logger.error("failed_to_relaunch_after_settings_import", {
            error: "[redacted]",
          });
          showErrorToast(t("settingsTransfer.restartError"));
        }
      }
    } catch (error) {
      logger.error("failed_to_import_settings", { error: "[redacted]" });
      showErrorToast(t("settingsTransfer.importError"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button
        ref={refs.setReference}
        type="button"
        aria-label={t("settingsTransfer.menu")}
        title={t("settingsTransfer.menu")}
        disabled={busy}
        className="flex h-9 w-9 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-backgroundHover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
        {...getReferenceProps()}
      >
        <DotsThree className="h-5 w-5" weight="bold" />
      </button>

      {open && (
        <FloatingPortal>
          <FloatingFocusManager context={context} modal={false}>
            <div
              ref={refs.setFloating}
              className="z-[9999] w-56 rounded-2xl border border-border bg-card p-1.5 shadow-lg outline-none"
              style={{ ...floatingStyles, pointerEvents: "auto" }}
              {...getFloatingProps()}
            >
              <button
                type="button"
                role="menuitem"
                className="flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-left text-sm text-foreground transition-colors hover:bg-backgroundHover focus-visible:bg-backgroundHover focus-visible:outline-none"
                onClick={handleExport}
              >
                <DownloadSimple className="h-4 w-4 text-muted-foreground" />
                <span>{t("settingsTransfer.export")}</span>
              </button>
              <button
                type="button"
                role="menuitem"
                className="flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-left text-sm text-foreground transition-colors hover:bg-backgroundHover focus-visible:bg-backgroundHover focus-visible:outline-none"
                onClick={handleImport}
              >
                <UploadSimple className="h-4 w-4 text-muted-foreground" />
                <span>{t("settingsTransfer.import")}</span>
              </button>
              <div
                role="none"
                className="border-t border-border/70 px-3 pb-1 pt-2 text-xs leading-5 text-muted-foreground"
              >
                <p>{t("settingsTransfer.secretNote")}</p>
              </div>
            </div>
          </FloatingFocusManager>
        </FloatingPortal>
      )}
    </>
  );
}
