/** Nomes comuns no Brasil para o botao "gerar nome" dos DOTs. */
export const COMMON_NAMES = [
  "Ana", "Maria", "Julia", "Beatriz", "Camila", "Fernanda", "Larissa", "Mariana", "Patricia", "Aline",
  "Bruna", "Carla", "Daniela", "Gabriela", "Juliana", "Leticia", "Luana", "Natalia", "Renata", "Vanessa",
  "Amanda", "Bianca", "Carolina", "Isabela", "Rafaela", "Sabrina", "Tatiane", "Vitoria", "Helena", "Laura",
  "Joao", "Jose", "Pedro", "Lucas", "Gabriel", "Rafael", "Bruno", "Carlos", "Daniel", "Diego",
  "Eduardo", "Felipe", "Gustavo", "Henrique", "Igor", "Leonardo", "Marcelo", "Mateus", "Paulo", "Ricardo",
  "Rodrigo", "Thiago", "Vinicius", "Andre", "Fabio", "Fernando", "Guilherme", "Leandro", "Marcos", "Vitor",
];

/** Nome comum sorteado que ninguem usa ainda (sem diferenciar maiusculas); `current` nao se repete em seguida. */
export function randomName(taken: string[], current = "", rand: () => number = Math.random): string {
  const used = new Set([...taken, current].map((n) => n.trim().toLowerCase()));
  const free = COMMON_NAMES.filter((n) => !used.has(n.toLowerCase()));
  const pool = free.length ? free : COMMON_NAMES;
  return pool[Math.floor(rand() * pool.length)]!;
}
