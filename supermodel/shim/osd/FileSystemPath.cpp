// Supermodel's file-system OSD: everything lives under one data directory, /supermodel in the
// Emscripten in-memory file system (libretro.cpp changes it for the native build).
#include "OSD/FileSystemPath.h"
#include <sys/stat.h>
#include <string>

static std::string s_base = "/supermodel";

extern "C" void sm_set_data_dir(const char *dir)
{
  s_base = dir;
}

namespace FileSystemPath
{
  bool PathExists(std::string fileSystemPath)
  {
    struct stat info;
    return stat(fileSystemPath.c_str(), &info) == 0 && S_ISDIR(info.st_mode);
  }

  int MakeDir(std::string dir)
  {
    return PathExists(dir) ? 0 : mkdir(dir.c_str(), 0775);
  }

  std::string GetPath(PathType pathType)
  {
    const char *name = "Misc";
    switch (pathType)
    {
    case Analysis: name = "Analysis"; break;
    case Config: name = "Config"; break;
    case Log: name = "Log"; break;
    case NVRAM: name = "NVRAM"; break;
    case Saves: name = "Saves"; break;
    case Screenshots: name = "Screenshots"; break;
    case Assets: name = "Assets"; break;
    }
    MakeDir(s_base);
    std::string path = s_base + "/" + name + "/";
    MakeDir(path);
    return path;
  }
}
