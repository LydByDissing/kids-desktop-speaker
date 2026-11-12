import { fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { App } from "./App";
import { MockClient, MockOptions } from "./api/mock";
import { fakeRecorder } from "./wav";

function start(options: MockOptions = {}, signedIn = true) {
  localStorage.setItem("kds-lang", "da");
  const client = new MockClient(options);
  render(<App client={client} recorder={fakeRecorder} signedIn={signedIn} origin="https://kidsspeaker-2df5.local" />);
  return client;
}

const type = (label: string, value: string) => fireEvent.input(screen.getByLabelText(label), { target: { value } });

afterEach(() => localStorage.clear());

describe("first-time setup (AUTH_REQ_003)", () => {
  it("shows the tries left after a wrong code, then opens the app", async () => {
    const client = start({ setUp: false, code: "471293" });
    await screen.findByText("Sæt højttaleren op");
    type("Kode", "000000");
    fireEvent.click(screen.getByText("Fortsæt"));
    await screen.findByText("Forkert kode. 4 forsøg tilbage.");
    type("Kode", "471293");
    type("Navn på denne enhed", "Tues telefon");
    fireEvent.click(screen.getByText("Fortsæt"));
    await screen.findByRole("heading", { name: "Afspiller" });
    expect(client.tokenList.at(-1)?.name).toBe("Tues telefon");
    expect(localStorage.getItem("kds-token")).toMatch(/^kds_mock_/);
    expect(screen.getAllByRole("button", { name: /Afspiller|Radio|Beskeder|Indstillinger/ }).length).toBeGreaterThanOrEqual(4);
  });
});

describe("a child's device (DEC-045)", () => {
  it("shows only the Player and Radio tabs", async () => {
    start({ admin: false });
    await screen.findByRole("heading", { name: "Afspiller" });
    const nav = screen.getByRole("navigation");
    expect(nav.textContent).toContain("Radio");
    expect(nav.textContent).not.toContain("Indstillinger");
    expect(nav.textContent).not.toContain("Beskeder");
  });
});

describe("the player (UI_REQ_002)", () => {
  it("plays a station and stops it", async () => {
    const client = start();
    await screen.findByText("Intet spiller");
    fireEvent.click(await screen.findByRole("button", { name: "DR P3" }));
    await screen.findByText("Radio · DR P3");
    expect(client.playing).toBe("dr-p3");
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    await screen.findByText("Intet spiller");
  });

  it("shows the volume with its limit", async () => {
    start({ volume: 40, limit: 40 });
    await screen.findByText("40 % · højst 40 %");
    expect((screen.getByLabelText("Lydstyrke") as HTMLInputElement).max).toBe("40");
  });
});

describe("voice messages (MSG_REQ_001)", () => {
  it("records, sends and lists a message", async () => {
    const client = start({ messages: [] });
    fireEvent.click(await screen.findByRole("button", { name: /Beskeder/ }));
    await screen.findByText("Ingen beskeder endnu.");
    fireEvent.click(screen.getByText(/Optag/));
    fireEvent.click(await screen.findByText(/Stop/));
    fireEvent.click(await screen.findByText("Send"));
    await screen.findByText(/\(uden navn\)/);
    expect(client.played).toHaveLength(1);
  });

  it("reads a text aloud in the chosen language", async () => {
    const client = start();
    fireEvent.click(await screen.findByRole("button", { name: /Beskeder/ }));
    type("Tekst", "Aftensmaden er klar");
    fireEvent.click(screen.getByText("Læs op"));
    await waitFor(() => expect(client.announced).toEqual([{ text: "Aftensmaden er klar", lang: "da-DK" }]));
  });
});

describe("sharing access (journey 6)", () => {
  it("shows the new token once with a QR code", async () => {
    const client = start();
    fireEvent.click(await screen.findByRole("button", { name: /Indstillinger/ }));
    type("Navn", "Idas iPad");
    fireEvent.click(screen.getByText("Ny adgang"));
    await screen.findByRole("img", { name: "QR-kode til adgang" });
    const created = client.tokenList.at(-1)!;
    expect(created.name).toBe("Idas iPad");
    expect(created.admin).toBe(false);
    expect(screen.getByText(/^kds_mock_/)).toBeTruthy();
  });
});

describe("language", () => {
  it("can be English", async () => {
    localStorage.setItem("kds-lang", "en");
    render(<App client={new MockClient()} recorder={fakeRecorder} signedIn origin="https://x" />);
    await screen.findByText("Nothing is playing");
  });
});

describe("live updates (UI_REQ_004, DEC-046)", () => {
  it("shows a change made elsewhere without reloading", async () => {
    const client = start();
    await screen.findByText("Intet spiller");
    // The radio starts from an alarm or another phone, not from this page.
    client.playing = "dr-p3";
    client.programme = "Go' Morgen P3";
    expect(await screen.findByText("Go' Morgen P3")).toBeTruthy();
    client.volumeValue = 30;
    expect(await screen.findByText(/^30 %/)).toBeTruthy();
  });

  it("shows the update reboot countdown on every screen (OTA_REQ_010)", async () => {
    start({ rebootIn: 300 });
    expect(await screen.findByText(/Højttaleren genstarter om (5:00|4:59)/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Radio/ }));
    expect(screen.getByText(/Højttaleren genstarter om/)).toBeTruthy();
  });

  it("restarts at once from the banner", async () => {
    const client = start({ rebootIn: 300 });
    let rebooted = false;
    client.otaReboot = async () => {
      rebooted = true;
    };
    await screen.findByText(/Højttaleren genstarter om/);
    fireEvent.click(screen.getAllByRole("button", { name: "Genstart nu" })[0]);
    await waitFor(() => expect(rebooted).toBe(true));
  });

  it("works without live updates", async () => {
    start({ live: false, playing: "dr-p1", programme: "P1 Morgen" });
    expect(await screen.findByText("P1 Morgen")).toBeTruthy();
    expect(screen.queryByText(/genstarter/)).toBeNull();
  });
});

