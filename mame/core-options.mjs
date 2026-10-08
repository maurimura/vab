// Core options for the scripts in mame/ (bench.mjs, link-check.mjs), on top of the ones
// web/emulator/libretro.js answers: it answers GET_VARIABLE only for its own keys, and the core
// takes the defaults of its option table (mame/patches/0002) for the rest. Call before
// `new Core(module, ...)`: the first "iii" function the frontend registers is its environment
// callback (retro_set_environment), which this wraps.
//
//   withOptions(module, { mame_drc: "enabled" })
//
// DRC=1 / DRC=0 in the environment: `optionsFromEnv()` turns it into mame_drc.
const GET_VARIABLE = 15;

export function optionsFromEnv(env = process.env) {
  const options = {};
  if (env.DRC !== undefined && env.DRC !== "") options.mame_drc = env.DRC === "1" ? "enabled" : "disabled";
  return options;
}

export function withOptions(module, options) {
  if (!Object.keys(options).length) return module;
  const strings = new Map();
  const cString = (text) => {
    let ptr = strings.get(text);
    if (!ptr) {
      const size = module.lengthBytesUTF8(text) + 1;
      ptr = module._malloc(size);
      module.stringToUTF8(text, ptr, size);
      strings.set(text, ptr);
    }
    return ptr;
  };
  const addFunction = module.addFunction;
  let wrapped = false;
  module.addFunction = (fn, signature) => {
    if (signature === "iii" && !wrapped) {
      wrapped = true;
      const environment = fn;
      fn = (cmd, data) => {
        if (cmd === GET_VARIABLE) {
          const key = module.UTF8ToString(module.getValue(data, "i32"));
          if (Object.hasOwn(options, key)) {
            module.setValue(data + 4, cString(options[key]), "i32");
            return 1;
          }
        }
        return environment(cmd, data);
      };
    }
    return addFunction(fn, signature);
  };
  return module;
}
