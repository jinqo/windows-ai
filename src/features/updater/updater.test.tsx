import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const downloadAndInstall = vi.fn(async (cb: (e: unknown) => void) => {
  cb({ event: "Started", data: { contentLength: 100 } });
  cb({ event: "Progress", data: { chunkLength: 100 } });
  cb({ event: "Finished" });
});
const relaunch = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => ({ version: "9.9.9", downloadAndInstall })) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));

import { UpdateBanner, useUpdater } from "./UpdateBanner";

describe("updater", () => {
  it("shows an available update and installs then relaunches", async () => {
    render(<UpdateBanner />);
    await useUpdater.getState().check(true);
    await waitFor(() => expect(screen.getByText("Windows AI 9.9.9 is available.")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Install & restart"));
    await waitFor(() => expect(relaunch).toHaveBeenCalled());
    expect(downloadAndInstall).toHaveBeenCalled();
  });
});
