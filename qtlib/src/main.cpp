#include <iostream>
#include <cstring>

#include "arrrpaperrust.h"

using namespace std;

int main() {
  char * from_rust = rust_generate_wallet(1, "user-provided-entropy");
  if (from_rust == nullptr) {
    cerr << "Wallet generation failed" << endl;
    return 1;
  }
  auto stri = string(from_rust);
  cout << stri << endl;
  rust_free_string(from_rust);

  return 0;
}
