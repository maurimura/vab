// Writes a no-op OpenGL ES 3.0 (gl_null.c) from Khronos' gl3.h, so the core runs where there is
// no GL: Node (bench.mjs) and the native baseline. Calls that hand out handles return nonzero ones
// and status queries succeed, so the renderers set themselves up and walk the scene each frame;
// nothing is drawn.
//   node gl_null.mjs <path to GLES3/gl3.h> > gl_null.c
import { readFileSync } from "node:fs";

const header = readFileSync(process.argv[2], "utf8");
const prototypes = /^GL_APICALL\s+(.+?)\s+GL_APIENTRY\s+(gl\w+)\s*\((.*)\);/gm;

let out = "#include <GLES3/gl3.h>\n\nint sm_gl_is_null = 1;\nstatic GLuint next_id = 1;\n\n";
for (const [, type, name, params] of header.matchAll(prototypes)) {
  const names = params === "void" ? [] : params.split(",").map((p) => p.trim().split(/[\s*]+/).pop());
  let body;
  if (name === "glCreateShader" || name === "glCreateProgram") body = "return next_id++;";
  else if (/^glGen[A-Z]/.test(name)) body = `for (GLsizei i = 0; i < ${names[0]}; i++) ${names[1]}[i] = next_id++;`;
  else if (name === "glGetShaderiv" || name === "glGetProgramiv")
    body = "*params = (pname == GL_COMPILE_STATUS || pname == GL_LINK_STATUS || pname == GL_VALIDATE_STATUS) ? GL_TRUE : 0;";
  else if (name === "glGetShaderInfoLog" || name === "glGetProgramInfoLog")
    body = "if (length) *length = 0; if (bufSize > 0) infoLog[0] = 0;";
  else if (name === "glCheckFramebufferStatus") body = "return GL_FRAMEBUFFER_COMPLETE;";
  else if (name === "glGetString" || name === "glGetStringi") body = 'return (const GLubyte *)"null";';
  else if (/^glGet(Integer|Float|Boolean|Integer64)v$/.test(name)) body = "*data = 0;";
  else if (name === "glUnmapBuffer") body = "return GL_TRUE;";
  else if (name === "glClientWaitSync") body = "return GL_ALREADY_SIGNALED;";
  else if (type === "void") body = "";
  else body = "return 0;";
  out += `${type} ${name}(${params})\n{\n  ${names.map((n) => `(void)${n};`).join(" ")}\n  ${body}\n}\n\n`;
}
// WebGL2-only, declared in GL/glew.h rather than gl3.h.
out += "void glGetBufferSubData(GLenum target, GLintptr offset, GLsizeiptr size, void *data)\n{\n  (void)target; (void)offset; (void)size; (void)data;\n}\n";
process.stdout.write(out);
