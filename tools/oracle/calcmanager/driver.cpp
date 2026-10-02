// Differential-testing oracle for the calcmanager Rust port.
// Licensed under the MIT License.
//
// Builds against the *unmodified* C++ CalcManager sources in
// reference/calculator/src/CalcManager (see build.sh) and replays
// deterministic pseudo-random operation sequences against
// CalculationManager::CalculatorManager, recording every ICalcDisplay
// callback. Output ("golden") format, one record per line:
//
//   #S <suite> <index> <seed>      start of a sequence (fresh manager, pristine statics)
//   > <op> [args...]               an operation that was executed
//   <tag>\t<fields...>             a callback / query result produced by that op
//
// Each sequence runs in a forked child (so every sequence starts from
// pristine process-wide statics, and crashes / UB-assertions / timeouts only
// discard that sequence). The generator inspects private engine state
// (via `#define private public`) to steer clear of inputs whose C++ behaviour
// is undefined (size_t underflow of the precedence stack, deref of a moved-out
// memory value, std::out_of_range on bad memory indices).

#include <algorithm>
#include <array>
#include <cassert>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <iomanip>
#include <iostream>
#include <list>
#include <memory>
#include <random>
#include <regex>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

#include <poll.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

#define private public
#define protected public
#include "CalculatorManager.h"
#include "CalculatorResource.h"
#undef private
#undef protected

using namespace CalculationManager;

// ---------------------------------------------------------------------------
// Strings (generated from crates/calcmanager/src/resource.rs by build.sh)
// ---------------------------------------------------------------------------

static const std::pair<const wchar_t*, const wchar_t*> kEngineStrings[] = {
#include "strings.inc"
};

class OracleResourceProvider : public IResourceProvider
{
public:
    // en-US separators by default; the "loc" suite uses a de/in-style mix.
    std::wstring decimal = L".", thousand = L",", grouping = L"3;0";

    std::wstring GetCEngineString(std::wstring_view id) override
    {
        if (id == L"sDecimal")
            return decimal;
        if (id == L"sThousand")
            return thousand;
        if (id == L"sGrouping")
            return grouping;
        for (auto const& kv : kEngineStrings)
        {
            if (id == kv.first)
                return kv.second;
        }
        return L"";
    }
};

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

static std::string utf8(std::wstring_view w)
{
    std::string out;
    for (wchar_t wc : w)
    {
        uint32_t c = static_cast<uint32_t>(wc);
        if (c < 0x80)
            out += static_cast<char>(c);
        else if (c < 0x800)
        {
            out += static_cast<char>(0xC0 | (c >> 6));
            out += static_cast<char>(0x80 | (c & 0x3F));
        }
        else if (c < 0x10000)
        {
            out += static_cast<char>(0xE0 | (c >> 12));
            out += static_cast<char>(0x80 | ((c >> 6) & 0x3F));
            out += static_cast<char>(0x80 | (c & 0x3F));
        }
        else
        {
            out += static_cast<char>(0xF0 | (c >> 18));
            out += static_cast<char>(0x80 | ((c >> 12) & 0x3F));
            out += static_cast<char>(0x80 | ((c >> 6) & 0x3F));
            out += static_cast<char>(0x80 | (c & 0x3F));
        }
    }
    return out;
}

// Escapes the field separators used by the golden format.
static std::string esc(std::string const& s)
{
    std::string out;
    for (char c : s)
    {
        switch (c)
        {
        case '\\':
            out += "\\\\";
            break;
        case '\t':
            out += "\\t";
            break;
        case '\n':
            out += "\\n";
            break;
        case '|':
            out += "\\p";
            break;
        case ';':
            out += "\\s";
            break;
        default:
            out += c;
        }
    }
    return out;
}

static std::string esc(std::wstring_view w)
{
    return esc(utf8(w));
}

static std::string hex32(uint32_t v)
{
    char buf[16];
    snprintf(buf, sizeof buf, "%08x", v);
    return buf;
}

static std::string join_ints(std::vector<int> const& v)
{
    std::string s;
    for (size_t i = 0; i < v.size(); i++)
    {
        if (i)
            s += ',';
        s += std::to_string(v[i]);
    }
    return s;
}

// Operand command lists are mostly digit ops; encode them compactly:
// IDC_0..IDC_F -> 0-9A-F, IDC_PNT -> '.', IDC_EXP -> 'e', IDC_SIGN -> '-',
// anything else -> {n}.
static std::string compact_opnd(std::vector<int> const& v)
{
    std::string s;
    for (int c : v)
    {
        if (c >= 130 && c <= 145)
            s += "0123456789ABCDEF"[c - 130];
        else if (c == 84)
            s += '.';
        else if (c == 127)
            s += 'e';
        else if (c == 80)
            s += '-';
        else
            s += "{" + std::to_string(c) + "}";
    }
    return s;
}

static std::string serialize_command(std::shared_ptr<IExpressionCommand> const& cmd)
{
    if (!cmd)
        return "NULL";
    switch (cmd->GetCommandType())
    {
    case CommandType::Parentheses:
        return "(" + std::to_string(std::static_pointer_cast<CParentheses>(cmd)->GetCommand());
    case CommandType::UnaryCommand:
        return "U" + join_ints(*std::static_pointer_cast<CUnaryCommand>(cmd)->GetCommands());
    case CommandType::BinaryCommand:
        return "B" + std::to_string(std::static_pointer_cast<CBinaryCommand>(cmd)->GetCommand());
    case CommandType::OperandCommand:
    {
        auto op = std::static_pointer_cast<COpndCommand>(cmd);
        std::string s = "O" + compact_opnd(*op->GetCommands()) + ":";
        s += op->IsNegative() ? '1' : '0';
        s += op->IsDecimalPresent() ? '1' : '0';
        s += op->IsSciFmt() ? '1' : '0';
        s += ':';
        try
        {
            s += esc(op->GetToken(L'.'));
        }
        catch (...)
        {
            s += "?";
        }
        return s;
    }
    }
    return "?";
}

static std::string serialize_commands(std::vector<std::shared_ptr<IExpressionCommand>> const* cmds)
{
    if (!cmds)
        return "NULL";
    std::string s;
    for (size_t i = 0; i < cmds->size(); i++)
    {
        if (i)
            s += ';';
        s += serialize_command((*cmds)[i]);
    }
    return s;
}

static std::string serialize_tokens(std::vector<std::pair<std::wstring, int>> const* toks)
{
    if (!toks)
        return "NULL";
    std::string s;
    for (size_t i = 0; i < toks->size(); i++)
    {
        if (i)
            s += '|';
        s += esc((*toks)[i].first) + "@" + std::to_string((*toks)[i].second);
    }
    return s;
}

// ---------------------------------------------------------------------------
// Recording display
// ---------------------------------------------------------------------------

class RecordingDisplay : public ICalcDisplay
{
public:
    std::string out;
    bool isInError = false;

    void SetPrimaryDisplay(const std::wstring& text, bool isError) override
    {
        out += "P\t" + esc(text) + "\t" + (isError ? "1" : "0") + "\n";
    }
    void SetIsInError(bool isError) override
    {
        isInError = isError;
        out += std::string("E\t") + (isError ? "1" : "0") + "\n";
    }
    void SetExpressionDisplay(
        std::shared_ptr<std::vector<std::pair<std::wstring, int>>> const& tokens,
        std::shared_ptr<std::vector<std::shared_ptr<IExpressionCommand>>> const& commands) override
    {
        out += "X\t" + serialize_tokens(tokens.get()) + "\t" + serialize_commands(commands.get()) + "\n";
    }
    void SetParenthesisNumber(unsigned int count) override
    {
        out += "N\t" + std::to_string(count) + "\n";
    }
    void OnNoRightParenAdded() override
    {
        out += "R\n";
    }
    void MaxDigitsReached() override
    {
        out += "D\n";
    }
    void BinaryOperatorReceived() override
    {
        out += "B\n";
    }
    void OnHistoryItemAdded(unsigned int idx) override
    {
        out += "H\t" + std::to_string(idx) + "\n";
    }
    void SetMemorizedNumbers(const std::vector<std::wstring>& nums) override
    {
        std::string s;
        for (size_t i = 0; i < nums.size(); i++)
        {
            if (i)
                s += '|';
            s += esc(nums[i]);
        }
        out += "M\t" + s + "\n";
    }
    void MemoryItemChanged(unsigned int idx) override
    {
        out += "C\t" + std::to_string(idx) + "\n";
    }
    void InputChanged() override
    {
        out += "I\n";
    }
};

// ---------------------------------------------------------------------------
// PRNG
// ---------------------------------------------------------------------------

struct Rng
{
    uint64_t s;
    explicit Rng(uint64_t seed)
        : s(seed)
    {
    }
    uint64_t next()
    {
        uint64_t z = (s += 0x9E3779B97F4A7C15ull);
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
        z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
        return z ^ (z >> 31);
    }
    uint32_t below(uint32_t n)
    {
        return n ? static_cast<uint32_t>(next() % n) : 0;
    }
    bool chance(double p)
    {
        return (next() >> 11) * (1.0 / 9007199254740992.0) < p;
    }
};

// ---------------------------------------------------------------------------
// Operation execution
// ---------------------------------------------------------------------------

struct Session
{
    RecordingDisplay display;
    OracleResourceProvider provider;
    std::unique_ptr<CalculatorManager> mgr;

    explicit Session(std::string const& suite)
    {
        if (suite == "loc")
        {
            provider.decimal = L",";
            provider.thousand = L".";
            provider.grouping = L"3;2;0";
        }
        mgr = std::make_unique<CalculatorManager>(&display, &provider);
    }

    void emit(std::string const& line)
    {
        display.out += line + "\n";
    }

    std::string history_dump(std::vector<std::shared_ptr<HISTORYITEM>> const& items)
    {
        std::string s = "=\t" + std::to_string(items.size()) + "\n";
        for (auto const& item : items)
        {
            auto const& v = item->historyItemVector;
            s += "=\t" + esc(v.expression) + "\t" + esc(v.result) + "\t" + serialize_tokens(v.spTokens.get()) + "\t"
                 + serialize_commands(v.spCommands.get()) + "\n";
        }
        return s;
    }

    // Executes one resolved op (already logged by the caller).
    void exec(std::vector<std::string> const& op)
    {
        auto const& k = op[0];
        auto arg = [&](size_t i) { return std::stoll(op.at(i)); };
        if (k == "SC")
            mgr->SendCommand(static_cast<Command>(arg(1)));
        else if (k == "MS")
            mgr->MemorizeNumber();
        else if (k == "ML")
            mgr->MemorizedNumberLoad(static_cast<unsigned>(arg(1)));
        else if (k == "MA")
            mgr->MemorizedNumberAdd(static_cast<unsigned>(arg(1)));
        else if (k == "MSUB")
            mgr->MemorizedNumberSubtract(static_cast<unsigned>(arg(1)));
        else if (k == "MC")
            mgr->MemorizedNumberClear(static_cast<unsigned>(arg(1)));
        else if (k == "MCA")
            mgr->MemorizedNumberClearAll();
        else if (k == "RESET")
            mgr->Reset(arg(1) != 0);
        else if (k == "STD")
            mgr->SetStandardMode();
        else if (k == "SCI")
            mgr->SetScientificMode();
        else if (k == "PROG")
            mgr->SetProgrammerMode();
        else if (k == "RADIX")
            mgr->SetRadix(static_cast<RadixType>(arg(1)));
        else if (k == "PREC")
            mgr->SetPrecision(static_cast<int32_t>(arg(1)));
        else if (k == "UMID")
            mgr->UpdateMaxIntDigits();
        else if (k == "MNS")
            mgr->SetMemorizedNumbersString();
        else if (k == "HLOAD")
            mgr->SetInHistoryItemLoadMode(arg(1) != 0);
        else if (k == "HRM")
            emit(std::string("=\t") + (mgr->RemoveHistoryItem(static_cast<unsigned>(arg(1))) ? "1" : "0"));
        else if (k == "HCLR")
            mgr->ClearHistory();
        else if (k == "HSET")
        {
            auto items = mgr->GetHistoryItems(static_cast<CalculatorMode>(arg(1)));
            mgr->SetHistoryItems(items);
        }
        else if (k == "PASTEERR")
            mgr->DisplayPasteError();
        else if (k == "GRR")
            emit("=\t" + esc(mgr->GetResultForRadix(static_cast<uint32_t>(arg(1)), static_cast<int32_t>(arg(2)), arg(3) != 0)));
        else if (k == "PANEL")
        {
            // StandardCalculatorViewModel::UpdateProgrammerPanelDisplay
            const int precision = 64;
            std::wstring hex, dec, oct, bin;
            if (!display.isInError)
            {
                hex = mgr->GetResultForRadix(16, precision, true);
                if (!hex.empty())
                {
                    dec = mgr->GetResultForRadix(10, precision, true);
                    oct = mgr->GetResultForRadix(8, precision, true);
                    bin = mgr->GetResultForRadix(2, precision, true);
                }
            }
            std::wstring raw = mgr->GetResultForRadix(2, precision, false);
            emit("=\t" + esc(hex) + "\t" + esc(dec) + "\t" + esc(oct) + "\t" + esc(bin) + "\t" + esc(raw));
        }
        else if (k == "REC")
            emit(std::string("=\t") + (mgr->IsEngineRecording() ? "1" : "0"));
        else if (k == "EMPTY")
            emit(std::string("=\t") + (mgr->IsInputEmpty() ? "1" : "0"));
        else if (k == "HIST")
            display.out += history_dump(mgr->GetHistoryItems());
        else if (k == "HISTM")
            display.out += history_dump(mgr->GetHistoryItems(static_cast<CalculatorMode>(arg(1))));
        else if (k == "SNAP")
        {
            auto snap = mgr->GetDisplayCommandsSnapshot();
            emit("=\t" + serialize_commands(&snap));
        }
        else if (k == "DEG")
            emit("=\t" + std::to_string(static_cast<int>(mgr->GetCurrentDegreeMode())));
        else if (k == "DSEP")
            emit("=\t" + esc(std::wstring(1, mgr->DecimalSeparator())));
        else if (k == "MAXH")
            emit("=\t" + std::to_string(mgr->MaxHistorySize()));
        else
        {
            fprintf(stderr, "unknown op %s\n", k.c_str());
            abort();
        }
    }

    void run(std::vector<std::string> const& op)
    {
        std::string line = ">";
        for (auto const& s : op)
            line += " " + s;
        emit(line);
        if (getenv("DRIVER_TRACE"))
        {
            fprintf(stderr, "%s\n", line.c_str());
            fflush(stderr);
        }
        try
        {
            exec(op);
        }
        catch (uint32_t e)
        {
            emit("!\t" + hex32(e));
        }
        catch (std::exception const& e)
        {
            emit(std::string("!\tstd:") + e.what());
        }
        catch (...)
        {
            emit("!\tunknown");
        }
    }
};

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

using Op = std::vector<std::string>;

static Op sc(int c)
{
    return { "SC", std::to_string(c) };
}

static int digit_cmd(int d)
{
    return static_cast<int>(Command::Command0) + d;
}

struct WeightedOp
{
    double w;
    std::function<void(std::vector<Op>&)> gen;
};

struct Generator
{
    std::string suite;
    Rng& rng;
    Session& s;

    Generator(std::string suite_, Rng& rng_, Session& s_)
        : suite(std::move(suite_))
        , rng(rng_)
        , s(s_)
    {
    }

    CCalcEngine* engine()
    {
        return s.mgr->m_currentCalculatorEngine;
    }

    bool is_programmer()
    {
        return engine() != nullptr && engine() == s.mgr->m_programmerCalculatorEngine.get();
    }

    // True when ")" / "=" cannot underflow the precedence stack (each open
    // paren still has its 0 marker) and cannot spin forever on a full stack.
    bool parens_safe()
    {
        CCalcEngine* e = engine();
        if (!e)
            return true;
        size_t count = std::min<size_t>(e->m_precedenceOpCount, MAXPRECDEPTH);
        size_t zeros = 0;
        for (size_t i = 0; i < count; i++)
            if (e->m_nPrecOp[i] == 0)
                zeros++;
        if (zeros < e->m_openParenCount)
            return false;
        if (e->m_openParenCount > 0 && e->m_precedenceOpCount >= MAXPRECDEPTH && e->m_nPrecOp[e->m_precedenceOpCount - 1] != 0)
            return false;
        return true;
    }

    Op guard(Op op)
    {
        // m_pHistory is null until standard/scientific mode was entered
        // (SetProgrammerMode never sets it): the C++ dereferences null.
        if ((op[0] == "HIST" || op[0] == "MAXH" || op[0] == "HRM" || op[0] == "HCLR" || op[0] == "HSET") && s.mgr->m_pHistory == nullptr)
            return { "HISTM", "0" };
        if (op[0] == "SC")
        {
            int c = std::stoi(op[1]);
            if ((c == static_cast<int>(Command::CommandCLOSEP) || c == static_cast<int>(Command::CommandEQU)) && !parens_safe())
                return sc(static_cast<int>(Command::CommandCLEAR));
            if (c == static_cast<int>(Command::CommandRECALL) || c == static_cast<int>(Command::CommandMPLUS)
                || c == static_cast<int>(Command::CommandMMINUS))
            {
                CCalcEngine* e = engine();
                if (e && e->m_memoryValue == nullptr)
                    return sc(static_cast<int>(Command::CommandSTORE));
            }
        }
        return op;
    }

    size_t mem_count()
    {
        return s.mgr->m_memorizedNumbers.size();
    }

    void digits(std::vector<Op>& out, int radix, int n)
    {
        for (int i = 0; i < n; i++)
        {
            int d = static_cast<int>(rng.below(radix));
            // Bias toward small digits / zeros for nicer numbers.
            if (rng.chance(0.15))
                d = 0;
            out.push_back(sc(digit_cmd(d)));
        }
    }

    void number(std::vector<Op>& out, int radix)
    {
        uint32_t r = rng.below(100);
        int len;
        if (r < 55)
            len = 1 + rng.below(2);
        else if (r < 85)
            len = 1 + rng.below(5);
        else if (r < 95)
            len = 5 + rng.below(12);
        else
            len = 15 + rng.below(25);
        digits(out, radix, len);
        if (radix == 10 && rng.chance(0.2))
        {
            out.push_back(sc(static_cast<int>(Command::CommandPNT)));
            digits(out, 10, 1 + rng.below(rng.chance(0.2) ? 30 : 4));
        }
    }

    void memory_op(std::vector<Op>& out)
    {
        size_t n = mem_count();
        uint32_t r = rng.below(10);
        if (r < 3 || n == 0)
        {
            if (n == 0 && r >= 3 && r < 8)
            {
                // Add/Subtract on empty memory memorize first.
                out.push_back({ r < 6 ? "MA" : "MSUB", "0" });
            }
            else
                out.push_back({ "MS" });
            return;
        }
        std::string idx = std::to_string(rng.below(static_cast<uint32_t>(n)));
        switch (r)
        {
        case 3:
        case 4:
            out.push_back({ "ML", idx });
            break;
        case 5:
            out.push_back({ "MA", idx });
            break;
        case 6:
            out.push_back({ "MSUB", idx });
            break;
        case 7:
            out.push_back({ "MC", idx });
            break;
        case 8:
            out.push_back({ "MCA" });
            break;
        default:
            out.push_back({ "MNS" });
            break;
        }
    }

    void query_op(std::vector<Op>& out)
    {
        static const char* q[] = { "REC", "EMPTY", "SNAP", "HIST", "DEG", "DSEP", "MAXH" };
        uint32_t r = rng.below(9);
        if (r < 7)
            out.push_back({ q[r] });
        else
            out.push_back({ "HISTM", std::to_string(rng.below(2)) });
    }

    std::vector<WeightedOp> table_common()
    {
        using C = Command;
        auto c = [](C x) { return static_cast<int>(x); };
        std::vector<WeightedOp> t;
        auto add = [&](double w, int cmd) { t.push_back({ w, [cmd](std::vector<Op>& o) { o.push_back(sc(cmd)); } }); };
        t.push_back({ 30, [this](std::vector<Op>& o) { number(o, 10); } });
        add(4, c(C::CommandADD));
        add(4, c(C::CommandSUB));
        add(4, c(C::CommandMUL));
        add(4, c(C::CommandDIV));
        add(6, c(C::CommandEQU));
        add(3, c(C::CommandSIGN));
        add(2, c(C::CommandPNT));
        add(2, c(C::CommandPERCENT));
        add(2, c(C::CommandSQRT));
        add(2, c(C::CommandSQR));
        add(2, c(C::CommandREC));
        add(1, c(C::CommandCLEAR));
        add(2, c(C::CommandCENTR));
        add(3, c(C::CommandBACK));
        t.push_back({ 3, [this](std::vector<Op>& o) { memory_op(o); } });
        t.push_back({ 1.5, [this](std::vector<Op>& o) { query_op(o); } });
        return t;
    }

    std::vector<WeightedOp> table_scientific()
    {
        using C = Command;
        auto c = [](C x) { return static_cast<int>(x); };
        std::vector<WeightedOp> t = table_common();
        auto add = [&](double w, int cmd) { t.push_back({ w, [cmd](std::vector<Op>& o) { o.push_back(sc(cmd)); } }); };
        add(5, c(C::CommandOPENP));
        add(5, c(C::CommandCLOSEP));
        t.push_back({ 0.8, [this](std::vector<Op>& o) {
                         // nesting burst
                         int n = 2 + static_cast<int>(rng.below(rng.chance(0.2) ? 26 : 6));
                         for (int i = 0; i < n; i++)
                             o.push_back(sc(static_cast<int>(Command::CommandOPENP)));
                     } });
        t.push_back({ 0.8, [this](std::vector<Op>& o) {
                         // precedence chain: a op b op c ... with mixed precedence levels
                         static const Command ops[] = { Command::CommandADD, Command::CommandSUB, Command::CommandMUL, Command::CommandDIV,
                                                        Command::CommandPWR, Command::CommandROOT, Command::CommandMOD, Command::CommandLogBaseY };
                         int n = 2 + static_cast<int>(rng.below(8));
                         for (int i = 0; i < n; i++)
                         {
                             digits(o, 10, 1 + static_cast<int>(rng.below(2)));
                             o.push_back(sc(static_cast<int>(ops[rng.below(8)])));
                         }
                     } });
        add(2, c(C::CommandPWR));
        add(1, c(C::CommandROOT));
        add(1.5, c(C::CommandMOD));
        add(1, c(C::CommandLogBaseY));
        for (C x : { C::CommandSIN, C::CommandCOS, C::CommandTAN })
            add(1, c(x));
        for (C x : { C::CommandSINH, C::CommandCOSH, C::CommandTANH, C::CommandSEC, C::CommandCSC, C::CommandCOT })
            add(0.5, c(x));
        for (C x : { C::CommandSECH, C::CommandCSCH, C::CommandCOTH })
            add(0.3, c(x));
        for (C x : { C::CommandASIN, C::CommandACOS, C::CommandATAN, C::CommandPOWE })
            add(0.6, c(x));
        for (C x : { C::CommandASINH, C::CommandACOSH, C::CommandATANH, C::CommandASEC, C::CommandACSC, C::CommandACOT, C::CommandASECH,
                     C::CommandACSCH, C::CommandACOTH })
            add(0.25, c(x));
        add(2, c(C::CommandINV));
        add(0.7, c(C::CommandDEG));
        add(0.7, c(C::CommandRAD));
        add(0.7, c(C::CommandGRAD));
        add(1, c(C::CommandFE));
        t.push_back({ 1.5, [this](std::vector<Op>& o) {
                         o.push_back(sc(static_cast<int>(Command::CommandEXP)));
                         if (rng.chance(0.4))
                             o.push_back(sc(static_cast<int>(Command::CommandSIGN)));
                         digits(o, 10, 1 + rng.below(5));
                     } });
        add(1, c(C::CommandPI));
        add(0.8, c(C::CommandEuler));
        add(1, c(C::CommandLN));
        add(1, c(C::CommandLOG));
        add(0.8, c(C::CommandPOW10));
        add(0.6, c(C::CommandPOW2));
        add(1, c(C::CommandFAC));
        add(0.6, c(C::CommandDMS));
        add(0.6, c(C::CommandDegrees));
        add(1, c(C::CommandCUB));
        add(0.6, c(C::CommandCUBEROOT));
        add(0.6, c(C::CommandAbs));
        add(0.6, c(C::CommandFloor));
        add(0.6, c(C::CommandCeil));
        add(0.5, c(C::CommandCHOP));
        add(0.3, c(C::CommandCOM));
        add(0.1, c(C::CommandSET_RESULT));
        for (C x : { C::CommandSTORE, C::CommandRECALL, C::CommandMPLUS, C::CommandMMINUS, C::CommandMCLEAR })
            add(0.25, c(x));
        return t;
    }

    std::vector<WeightedOp> table_programmer()
    {
        using C = Command;
        auto c = [](C x) { return static_cast<int>(x); };
        std::vector<WeightedOp> t;
        auto add = [&](double w, int cmd) { t.push_back({ w, [cmd](std::vector<Op>& o) { o.push_back(sc(cmd)); } }); };
        t.push_back({ 30, [this](std::vector<Op>& o) {
                         int radix = engine() ? static_cast<int>(engine()->m_radix) : 10;
                         if (rng.chance(0.1))
                             radix = 16; // some digits invalid in the current radix
                         uint32_t r = rng.below(100);
                         int len = r < 60 ? 1 + rng.below(3) : (r < 90 ? 1 + rng.below(8) : 8 + rng.below(60));
                         digits(o, radix, len);
                     } });
        for (C x : { C::CommandADD, C::CommandSUB, C::CommandMUL, C::CommandDIV, C::CommandMOD, C::CommandAnd, C::CommandOR, C::CommandXor,
                     C::CommandNand, C::CommandNor, C::CommandLSHF, C::CommandRSHF, C::CommandRSHFL })
            add(1.8, c(x));
        for (C x : { C::CommandROL, C::CommandROR, C::CommandROLC, C::CommandRORC, C::CommandCOM })
            add(1, c(x));
        add(6, c(C::CommandEQU));
        add(3, c(C::CommandSIGN));
        add(3, c(C::CommandBACK));
        add(2, c(C::CommandCENTR));
        add(1, c(C::CommandCLEAR));
        add(3, c(C::CommandOPENP));
        add(3, c(C::CommandCLOSEP));
        for (C x : { C::CommandHex, C::CommandDec, C::CommandOct, C::CommandBin })
            add(1.2, c(x));
        for (C x : { C::CommandQword, C::CommandDword, C::CommandWord, C::CommandByte })
            add(1.2, c(x));
        t.push_back({ 4, [this](std::vector<Op>& o) {
                         int bit = rng.chance(0.3) ? static_cast<int>(rng.below(8)) : static_cast<int>(rng.below(64));
                         o.push_back(sc(static_cast<int>(Command::CommandBINPOS0) + bit));
                     } });
        t.push_back({ 1, [this](std::vector<Op>& o) { o.push_back({ "RADIX", std::to_string(rng.below(4)) }); } });
        add(0.5, c(C::CommandPNT));
        add(0.5, c(C::CommandEXP));
        add(0.3, c(C::CommandPI));
        add(0.3, c(C::CommandPERCENT));
        add(0.3, c(C::CommandSQRT));
        add(0.3, c(C::CommandFAC));
        add(0.2, c(C::CommandSIN));
        for (C x : { C::CommandSTORE, C::CommandRECALL, C::CommandMPLUS, C::CommandMMINUS, C::CommandMCLEAR })
            add(0.25, c(x));
        t.push_back({ 3, [this](std::vector<Op>& o) { memory_op(o); } });
        t.push_back({ 1.5, [this](std::vector<Op>& o) { query_op(o); } });
        t.push_back({ 0.5, [this](std::vector<Op>& o) {
                         uint32_t radix[] = { 2, 8, 10, 16 };
                         o.push_back({ "GRR", std::to_string(radix[rng.below(4)]), std::to_string(rng.chance(0.8) ? 64 : 1 + rng.below(70)),
                                       std::to_string(rng.below(2)) });
                     } });
        return t;
    }

    std::vector<WeightedOp> table_misc()
    {
        std::vector<WeightedOp> t;
        auto add = [&](double w, Op op) { t.push_back({ w, [op](std::vector<Op>& o) { o.push_back(op); } }); };
        auto mode = [](Command c) { return sc(static_cast<int>(c)); };
        add(2, mode(Command::ModeBasic));
        add(2, mode(Command::ModeScientific));
        add(2, mode(Command::ModeProgrammer));
        add(1, { "STD" });
        add(1, { "SCI" });
        add(1, { "PROG" });
        add(0.7, { "RESET", "1" });
        add(0.7, { "RESET", "0" });
        add(1, { "HLOAD", "1" });
        add(1.5, { "HLOAD", "0" });
        t.push_back({ 1, [this](std::vector<Op>& o) { o.push_back({ "HRM", std::to_string(rng.below(4)) }); } });
        add(0.5, { "HCLR" });
        t.push_back({ 0.7, [this](std::vector<Op>& o) { o.push_back({ "HSET", std::to_string(rng.below(2)) }); } });
        add(0.7, { "PASTEERR" });
        t.push_back({ 0.7, [this](std::vector<Op>& o) {
                         int p[] = { 16, 32, 64, 8, 40 };
                         o.push_back({ "PREC", std::to_string(p[rng.below(5)]) });
                     } });
        add(0.5, { "UMID" });
        t.push_back({ 1, [this](std::vector<Op>& o) {
                         uint32_t radix[] = { 2, 8, 10, 16 };
                         o.push_back({ "GRR", std::to_string(radix[rng.below(4)]), std::to_string(rng.chance(0.7) ? 64 : 1 + rng.below(70)),
                                       std::to_string(rng.below(2)) });
                     } });
        return t;
    }

    static void pick(Rng& rng, std::vector<WeightedOp> const& t, std::vector<Op>& out)
    {
        double total = 0;
        for (auto const& w : t)
            total += w.w;
        double x = (rng.next() >> 11) * (1.0 / 9007199254740992.0) * total;
        for (auto const& w : t)
        {
            if (x < w.w)
            {
                w.gen(out);
                return;
            }
            x -= w.w;
        }
        t.back().gen(out);
    }

    // Produces the next batch of ops for the current state.
    std::vector<Op> step()
    {
        std::vector<Op> out;
        if (suite == "std")
            pick(rng, table_common(), out);
        else if (suite == "sci")
            pick(rng, rng.chance(0.85) ? table_scientific() : table_common(), out);
        else if (suite == "prog")
        {
            pick(rng, table_programmer(), out);
        }
        else // mix, loc
        {
            if (rng.chance(0.08))
                pick(rng, table_misc(), out);
            else if (is_programmer())
                pick(rng, table_programmer(), out);
            else
                pick(rng, table_scientific(), out);
        }
        return out;
    }
};

static std::string run_sequence(std::string const& suite, uint64_t seed)
{
    Rng rng(seed);
    Session s(suite);
    Generator g(suite, rng, s);

    // Start-up: put the manager in a mode the way the app / tests do.
    uint32_t start = rng.below(3);
    if (suite == "prog")
    {
        s.run(start == 0 ? Op{ "RESET", "1" } : Op{ "PROG" });
        if (start == 0)
            s.run(sc(static_cast<int>(Command::ModeProgrammer)));
    }
    else if (suite == "sci")
    {
        s.run(start == 0 ? Op{ "RESET", "1" } : Op{ "SCI" });
        if (start == 0)
            s.run(sc(static_cast<int>(Command::ModeScientific)));
    }
    else
    {
        s.run(start == 0 ? Op{ "RESET", "1" } : (start == 1 ? sc(static_cast<int>(Command::ModeBasic)) : Op{ "STD" }));
    }

    int n = 12 + static_cast<int>(rng.below(36));
    for (int i = 0; i < n; i++)
    {
        for (auto const& op : g.step())
        {
            s.run(g.guard(op));
            // The C# UI refreshes the programmer panel after every display update.
            if (g.is_programmer() && op[0] == "SC" && rng.chance(0.35))
                s.run({ "PANEL" });
        }
    }

    // Final state dump.
    for (const char* q : { "REC", "EMPTY", "SNAP", "HIST" })
        s.run(g.guard({ q }));
    return s.display.out;
}

// ---------------------------------------------------------------------------
// main: fork one child per sequence
// ---------------------------------------------------------------------------

// Replays the "> op" lines of a file in a single session (debugging aid):
//   driver replay <file> [suite]
static int replay_file(char const* path, std::string const& suite)
{
    FILE* f = fopen(path, "r");
    if (!f)
        return 1;
    Session s(suite);
    char line[65536];
    while (fgets(line, sizeof line, f))
    {
        std::string l(line);
        while (!l.empty() && (l.back() == '\n' || l.back() == '\r'))
            l.pop_back();
        if (l.rfind("> ", 0) != 0)
            continue;
        Op op;
        std::stringstream ss(l.substr(2));
        std::string w;
        while (ss >> w)
            op.push_back(w);
        auto t0 = std::chrono::steady_clock::now();
        s.run(op);
        double secs = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
        if (secs > 0.1)
            fprintf(stderr, "%.2fs %s\n", secs, l.c_str());
    }
    fclose(f);
    fwrite(s.display.out.data(), 1, s.display.out.size(), stdout);
    return 0;
}

int main(int argc, char** argv)
{
    if (argc >= 3 && std::string(argv[1]) == "replay")
        return replay_file(argv[2], argc >= 4 ? argv[3] : "std");
    if (argc != 5)
    {
        fprintf(stderr, "usage: %s <std|sci|prog|mix|loc> <first-index> <count> <base-seed>\n", argv[0]);
        return 2;
    }
    std::string suite = argv[1];
    uint64_t first = std::stoull(argv[2]);
    uint64_t count = std::stoull(argv[3]);
    uint64_t base = std::stoull(argv[4]);
    const double max_seconds = 2.0;

    size_t kept = 0, discarded = 0;
    for (uint64_t i = first; i < first + count; i++)
    {
        uint64_t suite_hash = 1469598103934665603ull; // FNV-1a
        for (char ch : suite)
            suite_hash = (suite_hash ^ static_cast<unsigned char>(ch)) * 1099511628211ull;
        uint64_t seed = base * 1000003ull + i * 7919ull + suite_hash % 100000;
        int fds[2];
        if (pipe(fds) != 0)
            return 1;
        auto t0 = std::chrono::steady_clock::now();
        pid_t pid = fork();
        if (pid == 0)
        {
            close(fds[0]);
            alarm(10);
            std::string out = "#S " + suite + " " + std::to_string(i) + " " + std::to_string(seed) + "\n" + run_sequence(suite, seed);
            size_t off = 0;
            while (off < out.size())
            {
                ssize_t w = write(fds[1], out.data() + off, out.size() - off);
                if (w <= 0)
                    _exit(3);
                off += static_cast<size_t>(w);
            }
            close(fds[1]);
            _exit(0);
        }
        close(fds[1]);
        std::string buf;
        char tmp[65536];
        for (;;)
        {
            ssize_t r = read(fds[0], tmp, sizeof tmp);
            if (r <= 0)
                break;
            buf.append(tmp, static_cast<size_t>(r));
        }
        close(fds[0]);
        int status = 0;
        waitpid(pid, &status, 0);
        double secs = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
        if (WIFEXITED(status) && WEXITSTATUS(status) == 0 && secs <= max_seconds)
        {
            fwrite(buf.data(), 1, buf.size(), stdout);
            kept++;
        }
        else
        {
            discarded++;
            fprintf(stderr, "discarded %s #%llu seed=%llu (%s, %.2fs)\n", suite.c_str(), static_cast<unsigned long long>(i),
                    static_cast<unsigned long long>(seed),
                    WIFSIGNALED(status) ? strsignal(WTERMSIG(status)) : (secs > max_seconds ? "slow" : "exit"), secs);
        }
    }
    fflush(stdout);
    fprintf(stderr, "%s: kept %zu, discarded %zu\n", suite.c_str(), kept, discarded);
    return 0;
}
