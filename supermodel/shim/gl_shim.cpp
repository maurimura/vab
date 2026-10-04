// The desktop OpenGL calls Supermodel makes that OpenGL ES 3.0 / WebGL2 lacks, and the rewrite
// of its GLSL 4.10 shaders into GLSL ES 3.00 (see GL/glew.h for how they are hooked in).
#include <GLES3/gl3.h>
#include <string>

extern "C" void sm_glDrawBuffer(GLenum buf)
{
  glDrawBuffers(1, &buf);
}

// GLSL ES needs the version line and default precisions; the rest of Supermodel's shaders is
// written to the common subset, except the sampler-array indexing patched in patches/.
static const char kHeader[] =
    "#version 300 es\n"
    "precision highp float;\n"
    "precision highp int;\n"
    "precision highp sampler2D;\n"
    "precision highp usampler2D;\n"
    "precision highp isampler2D;\n";

extern "C" void sm_glShaderSource(GLuint shader, GLsizei count, const GLchar *const *strings, const GLint *lengths)
{
  std::string source;
  for (GLsizei i = 0; i < count; i++)
  {
    if (!strings[i]) continue;
    if (lengths && lengths[i] >= 0) source.append(strings[i], (size_t)lengths[i]);
    else source.append(strings[i]);
  }
  // WebGL wants #version on the very first line; Supermodel's raw strings start with blank lines.
  size_t at = source.find("#version");
  if (at != std::string::npos)
  {
    size_t eol = source.find('\n', at);
    source.replace(0, (eol == std::string::npos ? source.size() : eol), kHeader);
  }
  else
  {
    source.insert(0, kHeader);
  }
  const GLchar *text = source.c_str();
  GLint length = (GLint)source.size();
  glShaderSource(shader, 1, &text, &length);
}
