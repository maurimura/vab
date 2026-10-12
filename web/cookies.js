// Cookies remember things for a year: the player's name and look (web/onboarding.js, /name in
// the chat), that the controls were shown (client/src/help.rs), who they muted, and the language
// the staff talk to them in (client/src/talk).

export const cookie = (key) => {
  const found = document.cookie.split("; ").find((part) => part.startsWith(`${key}=`));
  return found && decodeURIComponent(found.slice(key.length + 1));
};

export const remember = (key, value) => {
  document.cookie = `${key}=${encodeURIComponent(value)}; max-age=${365 * 24 * 3600}; path=/; samesite=lax`;
};
