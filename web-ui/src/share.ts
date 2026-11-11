import qrcode from "qrcode-generator";

/** The link that signs another browser in with a token. The token is in the
 *  URL fragment, which browsers never send to the server (DEC-045). */
export function accessLink(origin: string, token: string): string {
  return `${origin}/#token=${encodeURIComponent(token)}`;
}

/** Reads a token from a link's fragment, if there is one. */
export function tokenFromHash(hash: string): string | null {
  const match = /(?:^#|&)token=([^&]+)/.exec(hash);
  return match ? decodeURIComponent(match[1]) : null;
}

/** An SVG QR code for `text`. */
export function qrSvg(text: string): string {
  const qr = qrcode(0, "M");
  qr.addData(text);
  qr.make();
  return qr.createSvgTag({ cellSize: 4, margin: 2, scalable: true });
}
