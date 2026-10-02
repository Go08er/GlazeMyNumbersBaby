// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Differential-testing oracle for the Rust port of Ratpack (crates/ratpack).
//
// Reads one command per line on stdin, evaluates it with the ORIGINAL C++
// CalcEngine::Rational / RationalMath / ratpak code and prints
//
//     <command>\t<result>
//
// per line (flushed after every line, so a crash or hang points at the
// culprit). The output is the golden file replayed by
// crates/ratpack/tests/golden.rs; the command grammar is documented there.
//
// Build/run: see build.sh / regen.sh next to this file.

#include <chrono>
#include <cstdio>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

#include "Header Files/Rational.h"
#include "Header Files/RationalMath.h"

using namespace CalcEngine;
using namespace CalcEngine::RationalMath;

namespace
{
    std::map<std::string, Rational> g_regs;

    std::vector<std::string> split(const std::string& s, char sep)
    {
        std::vector<std::string> out;
        std::string cur;
        for (char c : s)
        {
            if (c == sep)
            {
                out.push_back(cur);
                cur.clear();
            }
            else
            {
                cur += c;
            }
        }
        out.push_back(cur);
        return out;
    }

    // "sign:exp:d0,d1,..."  (digits least significant first, base 2^31)
    Number parseNum(const std::string& s)
    {
        auto parts = split(s, ':');
        if (parts.size() != 3)
        {
            throw std::runtime_error("bad number " + s);
        }
        std::vector<uint32_t> mant;
        if (!parts[2].empty())
        {
            for (auto& d : split(parts[2], ','))
            {
                mant.push_back(static_cast<uint32_t>(std::stoul(d)));
            }
        }
        return Number(std::stoi(parts[0]), std::stoi(parts[1]), mant);
    }

    // "P/Q" or "$reg"
    Rational parseRat(const std::string& s)
    {
        if (!s.empty() && s[0] == '$')
        {
            auto it = g_regs.find(s.substr(1));
            if (it == g_regs.end())
            {
                throw std::runtime_error("unknown register " + s);
            }
            return it->second;
        }
        auto parts = split(s, '/');
        if (parts.size() != 2)
        {
            throw std::runtime_error("bad rational " + s);
        }
        return Rational(parseNum(parts[0]), parseNum(parts[1]));
    }

    std::string fmtNum(const Number& n)
    {
        std::ostringstream o;
        o << n.Sign() << ':' << n.Exp() << ':';
        bool first = true;
        for (auto d : n.Mantissa())
        {
            if (!first)
            {
                o << ',';
            }
            first = false;
            o << d;
        }
        return o.str();
    }

    std::string fmtRat(const Rational& r)
    {
        return "R:" + fmtNum(r.P()) + "/" + fmtNum(r.Q());
    }

    std::string fmtErr(uint32_t e)
    {
        char buf[32];
        snprintf(buf, sizeof(buf), "E:%08X", e);
        return buf;
    }

    // wstring results are ASCII except for a possible non-ASCII decimal
    // separator; emit UTF-8.
    std::string narrow(const std::wstring& w)
    {
        std::string out;
        for (wchar_t wc : w)
        {
            uint32_t c = static_cast<uint32_t>(wc);
            if (c < 0x80)
            {
                out += static_cast<char>(c);
            }
            else if (c < 0x800)
            {
                out += static_cast<char>(0xC0 | (c >> 6));
                out += static_cast<char>(0x80 | (c & 0x3F));
            }
            else
            {
                out += static_cast<char>(0xE0 | (c >> 12));
                out += static_cast<char>(0x80 | ((c >> 6) & 0x3F));
                out += static_cast<char>(0x80 | (c & 0x3F));
            }
        }
        return out;
    }

    // '...' quoted string, no escapes; '' is empty.
    std::wstring unquote(const std::string& s)
    {
        if (s.size() < 2 || s.front() != '\'' || s.back() != '\'')
        {
            throw std::runtime_error("bad string " + s);
        }
        std::wstring w;
        for (size_t i = 1; i + 1 < s.size(); i++)
        {
            w += static_cast<wchar_t>(static_cast<unsigned char>(s[i]));
        }
        return w;
    }

    NumberFormat parseFmt(const std::string& s)
    {
        if (s == "float")
            return NumberFormat::Float;
        if (s == "sci")
            return NumberFormat::Scientific;
        if (s == "eng")
            return NumberFormat::Engineering;
        throw std::runtime_error("bad format " + s);
    }

    AngleType parseAngle(const std::string& s)
    {
        if (s == "deg")
            return AngleType::Degrees;
        if (s == "rad")
            return AngleType::Radians;
        if (s == "grad")
            return AngleType::Gradians;
        throw std::runtime_error("bad angle " + s);
    }

    Rational binop(const std::string& op, const Rational& a, const Rational& b)
    {
        if (op == "add")
            return a + b;
        if (op == "sub")
            return a - b;
        if (op == "mul")
            return a * b;
        if (op == "div")
            return a / b;
        if (op == "rem")
            return a % b;
        if (op == "shl")
            return a << b;
        if (op == "shr")
            return a >> b;
        if (op == "and")
            return a & b;
        if (op == "or")
            return a | b;
        if (op == "xor")
            return a ^ b;
        if (op == "mod")
            return Mod(a, b);
        if (op == "pow")
            return Pow(a, b);
        if (op == "root")
            return Root(a, b);
        throw std::runtime_error("bad binop " + op);
    }

    Rational unop(const std::string& op, const Rational& a)
    {
        if (op == "neg")
            return -a;
        if (op == "frac")
            return Frac(a);
        if (op == "int")
            return Integer(a);
        if (op == "fact")
            return Fact(a);
        if (op == "exp")
            return Exp(a);
        if (op == "log")
            return Log(a);
        if (op == "log10")
            return Log10(a);
        if (op == "inv")
            return Invert(a);
        if (op == "abs")
            return Abs(a);
        if (op == "sinh")
            return Sinh(a);
        if (op == "cosh")
            return Cosh(a);
        if (op == "tanh")
            return Tanh(a);
        if (op == "asinh")
            return ASinh(a);
        if (op == "acosh")
            return ACosh(a);
        if (op == "atanh")
            return ATanh(a);
        throw std::runtime_error("bad unop " + op);
    }

    Rational trigop(const std::string& op, AngleType at, const Rational& a)
    {
        if (op == "sin")
            return Sin(a, at);
        if (op == "cos")
            return Cos(a, at);
        if (op == "tan")
            return Tan(a, at);
        if (op == "asin")
            return ASin(a, at);
        if (op == "acos")
            return ACos(a, at);
        if (op == "atan")
            return ATan(a, at);
        throw std::runtime_error("bad trigop " + op);
    }

    Rational constant(const std::string& name)
    {
        if (name == "qword")
            return Rational{ rat_qword };
        if (name == "dword")
            return Rational{ rat_dword };
        if (name == "word")
            return Rational{ rat_word };
        if (name == "byte")
            return Rational{ rat_byte };
        if (name == "exp")
            return Rational{ rat_exp };
        if (name == "ln_ten")
            return Rational{ ln_ten };
        if (name == "pi")
            return Rational{ pi };
        throw std::runtime_error("bad constant " + name);
    }

    // Executes one command (without any "@reg=" prefix). Returns the result
    // text; *ratOut receives a rational result (for register stores).
    std::string execute(const std::vector<std::string>& t, Rational* ratOut, bool* haveRat)
    {
        const std::string& cmd = t.at(0);
        *haveRat = false;
        auto ratResult = [&](const Rational& r) {
            *ratOut = r;
            *haveRat = true;
            return fmtRat(r);
        };

        if (cmd == "CC")
        {
            ChangeConstants(static_cast<uint32_t>(std::stoul(t.at(1))), std::stoi(t.at(2)));
            return "ok";
        }
        if (cmd == "SEP")
        {
            std::wstring w = unquote(t.at(1));
            SetDecimalSeparator(w.at(0));
            return "ok";
        }
        if (cmd == "BIN")
        {
            return ratResult(binop(t.at(1), parseRat(t.at(2)), parseRat(t.at(3))));
        }
        if (cmd == "UN")
        {
            return ratResult(unop(t.at(1), parseRat(t.at(2))));
        }
        if (cmd == "TRIG")
        {
            return ratResult(trigop(t.at(1), parseAngle(t.at(2)), parseRat(t.at(3))));
        }
        if (cmd == "CMP")
        {
            Rational a = parseRat(t.at(1));
            Rational b = parseRat(t.at(2));
            std::ostringstream o;
            o << "C:" << (a == b) << (a != b) << (a < b) << (a > b) << (a <= b) << (a >= b);
            return o.str();
        }
        if (cmd == "STR")
        {
            uint32_t radix = static_cast<uint32_t>(std::stoul(t.at(1)));
            NumberFormat fmt = parseFmt(t.at(2));
            int32_t precision = std::stoi(t.at(3));
            return "S:" + narrow(parseRat(t.at(4)).ToString(radix, fmt, precision));
        }
        if (cmd == "U64")
        {
            return "U:" + std::to_string(parseRat(t.at(1)).ToUInt64_t());
        }
        if (cmd == "FROMI32")
        {
            return ratResult(Rational(static_cast<int32_t>(std::stol(t.at(1)))));
        }
        if (cmd == "FROMU32")
        {
            return ratResult(Rational(static_cast<uint32_t>(std::stoul(t.at(1)))));
        }
        if (cmd == "FROMU64")
        {
            return ratResult(Rational(static_cast<uint64_t>(std::stoull(t.at(1)))));
        }
        if (cmd == "FROMNUM")
        {
            return ratResult(Rational(parseNum(t.at(1))));
        }
        if (cmd == "CONST")
        {
            return ratResult(constant(t.at(1)));
        }
        if (cmd == "S2R")
        {
            // S2R <mantNeg 0|1> '<mant>' <expNeg 0|1> '<exp>' <radix> <precision>
            std::wstring mant = unquote(t.at(2));
            std::wstring expo = unquote(t.at(4));
            PRAT rat = StringToRat(t.at(1) == "1", mant, t.at(3) == "1", expo, static_cast<uint32_t>(std::stoul(t.at(5))), std::stoi(t.at(6)));
            if (rat == nullptr)
            {
                return "NULL";
            }
            Rational r{ rat };
            destroyrat(rat);
            return ratResult(r);
        }
        throw std::runtime_error("bad command " + cmd);
    }
}

int main(int argc, char** argv)
{
    // Mirror the calculator: CCalcEngine::InitialOneTimeOnlySetup ->
    // ChangeBaseConstants(10, 32, 32) -> ChangeConstants(10, 32).
    ChangeConstants(10, 32);

    double slowMs = argc > 1 ? std::stod(argv[1]) : 1e18;

    std::string line;
    while (std::getline(std::cin, line))
    {
        if (line.empty() || line[0] == '#')
        {
            continue;
        }
        std::vector<std::string> tokens;
        {
            std::istringstream in(line);
            std::string tok;
            while (in >> tok)
            {
                tokens.push_back(tok);
            }
        }
        std::string reg;
        if (!tokens.empty() && tokens[0][0] == '@')
        {
            reg = tokens[0].substr(1);
            tokens.erase(tokens.begin());
        }

        auto start = std::chrono::steady_clock::now();
        std::string result;
        Rational value;
        bool haveRat = false;
        try
        {
            result = execute(tokens, &value, &haveRat);
        }
        catch (uint32_t e)
        {
            result = fmtErr(e);
        }
        catch (std::exception& e)
        {
            std::cerr << "driver error: " << e.what() << " in: " << line << std::endl;
            return 2;
        }
        auto ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
        if (ms > slowMs)
        {
            std::cerr << "SLOW " << ms << "ms: " << line << std::endl;
        }

        if (!reg.empty())
        {
            // Errors store nothing; later uses of the register then fail
            // loudly in the generator's sanity pass.
            if (haveRat)
            {
                g_regs[reg] = value;
            }
        }
        std::cout << line << '\t' << result << '\n' << std::flush;
    }
    return 0;
}
