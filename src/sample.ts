// Sample TypeScript file with intentional lint errors

const unusedVariable = "this is never used";

function greet(name: string) {
  
  var x = 42;
  if (name == "world") {
    console.log("Hello, " + name);
  }
  return x;
}

const result = greet("world");
console.log(result);
// trigger hook test v6 - full linter hook
var unused = 123;

console.log(void 0 === undefined);
