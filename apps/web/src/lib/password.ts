/** Mirrors `password::MIN_PASSWORD_LEN`; the server is still the authority. */
export const MIN_PASSWORD_LEN = 10;

/** The one wording for the rules, shared by every place a password is chosen. */
export const PASSWORD_RULES = `At least ${MIN_PASSWORD_LEN} characters. A short phrase works well.`;
