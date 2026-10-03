/** The core's minimum length of a master or backup password, in characters (code points, as the
 *  core counts them, so a Chinese pass phrase is not cut short). */
export const MIN_PASSWORD = 8;

export function passwordLongEnough(password: string): boolean {
  return Array.from(password).length >= MIN_PASSWORD;
}
