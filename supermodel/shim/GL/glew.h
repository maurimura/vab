// Stand-in for GLEW. Supermodel's renderers include <GL/glew.h> for desktop OpenGL; here they get
// OpenGL ES 3.0, which is what WebGL2 exposes through Emscripten, plus the few desktop-only calls
// they make, mapped onto ES in gl_shim.cpp. The headless and native builds link a no-op GL
// (gl_null.c) behind the same header.
#pragma once
#include <GLES3/gl3.h>
#include <GLES2/gl2ext.h>
#include <stddef.h>

#ifndef GLEW_OK
#define GLEW_OK 0
#endif
typedef double GLdouble;
static inline GLenum glewInit(void) { return GLEW_OK; }
static inline const GLubyte *glewGetErrorString(GLenum error) { (void)error; return (const GLubyte *)""; }
static inline const GLubyte *glewGetString(GLenum name) { (void)name; return (const GLubyte *)""; }

// Only Supermodel's quad renderer uses geometry shaders; it stays off here (QuadRendering=false).
#ifndef GL_GEOMETRY_SHADER
#define GL_GEOMETRY_SHADER 0x8DD9
#endif
#ifndef GL_LINES_ADJACENCY
#define GL_LINES_ADJACENCY 0x000A
#endif

#ifdef __cplusplus
extern "C" {
#endif
void sm_glDrawBuffer(GLenum buf);
void sm_glShaderSource(GLuint shader, GLsizei count, const GLchar *const *string, const GLint *length);
#ifdef __cplusplus
}
#endif

#define glDrawBuffer(buf) sm_glDrawBuffer(buf)
#define glClearDepth(depth) glClearDepthf((GLfloat)(depth))
#define glShaderSource sm_glShaderSource
