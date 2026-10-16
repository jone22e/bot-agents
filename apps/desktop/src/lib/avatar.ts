// Imagem de perfil de cada DOT: personagem de desenho gerado no proprio app (DiceBear, estilo Avataaars, de uso
// livre inclusive comercial). A semente e o id do DOT: cada um ganha o seu boneco ao ser criado e ele nunca muda.
import { createAvatar } from "@dicebear/core";
import { avataaars } from "@dicebear/collection";

const BACKGROUNDS = ["b6e3f4", "c0aede", "d1d4f9", "ffd5dc", "ffdfbf", "c7f0d8"];
const cache = new Map<string, string>();

/** Imagem (data URI SVG) do DOT com esta semente. */
export function avatarUri(seed: string): string {
  let uri = cache.get(seed);
  if (!uri) {
    uri = createAvatar(avataaars, { seed, backgroundColor: BACKGROUNDS, backgroundType: ["solid"] }).toDataUri();
    cache.set(seed, uri);
  }
  return uri;
}
