import { qrSvg } from "../share";

/** A QR code for `text`, drawn as SVG. */
export function QrCode({ text, label }: { text: string; label: string }) {
  return <div class="qr" role="img" aria-label={label} dangerouslySetInnerHTML={{ __html: qrSvg(text) }} />;
}
