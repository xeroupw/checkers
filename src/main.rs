use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, ClearType},
};
use std::io::{self, Write};
use std::time::Duration;

const SIZE: usize = 8;

// checkers rule variant
#[derive(Clone, Copy, PartialEq, Debug)]
enum Variant {
    Russian,
    English,
    Brazilian,
    Turkish,
}

impl Variant {
    // king slides multiple squares along a diagonal
    fn flying_king(&self) -> bool {
        matches!(self, Variant::Russian | Variant::Brazilian)
    }

    // regular pieces may capture backwards
    fn backward_capture(&self) -> bool {
        matches!(self, Variant::Russian | Variant::Brazilian)
    }

    // pieces move orthogonally instead of diagonally
    fn orthogonal(&self) -> bool {
        matches!(self, Variant::Turkish)
    }

    fn label(&self) -> &'static str {
        match self {
            Variant::Russian => "Russian",
            Variant::English => "English (Draughts)",
            Variant::Brazilian => "Brazilian",
            Variant::Turkish => "Turkish",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Variant::Russian => "Flying kings  *  backward captures",
            Variant::English => "Short kings  *  forward captures only",
            Variant::Brazilian => "Flying kings  *  backward captures",
            Variant::Turkish => "Orthogonal moves  *  no diagonal",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Piece {
    None,
    White,
    Black,
    WhiteKing,
    BlackKing,
}

impl Piece {
    fn is_white(&self) -> bool {
        matches!(self, Piece::White | Piece::WhiteKing)
    }

    fn is_black(&self) -> bool {
        matches!(self, Piece::Black | Piece::BlackKing)
    }

    fn is_king(&self) -> bool {
        matches!(self, Piece::WhiteKing | Piece::BlackKing)
    }

    fn belongs_to(&self, white_turn: bool) -> bool {
        if white_turn {
            self.is_white()
        } else {
            self.is_black()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Pos {
    row: usize,
    col: usize,
}

impl Pos {
    fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

struct Game {
    board: [[Piece; SIZE]; SIZE],
    cursor: Pos,
    selected: Option<Pos>,
    white_turn: bool,
    valid_moves: Vec<(Pos, Option<Pos>)>,
    must_continue: Option<Pos>,
    status_msg: String,
    white_count: u8,
    black_count: u8,
    game_over: bool,
    bot_thinking: bool,
    variant: Variant,
    needs_clear: bool,
}

impl Game {
    fn new(variant: Variant) -> Self {
        let mut board = [[Piece::None; SIZE]; SIZE];
        if variant.orthogonal() {
            // turkish: pieces on rows 1-2 (black) and 5-6 (white), all columns
            for row in 1..3 {
                for col in 0..SIZE {
                    board[row][col] = Piece::Black;
                }
            }
            for row in 5..7 {
                for col in 0..SIZE {
                    board[row][col] = Piece::White;
                }
            }
        } else {
            for row in 0..3 {
                for col in 0..SIZE {
                    if (row + col) % 2 == 1 {
                        board[row][col] = Piece::Black;
                    }
                }
            }
            for row in 5..SIZE {
                for col in 0..SIZE {
                    if (row + col) % 2 == 1 {
                        board[row][col] = Piece::White;
                    }
                }
            }
        }
        Self {
            board,
            cursor: Pos::new(5, 0),
            selected: None,
            white_turn: true,
            valid_moves: Vec::new(),
            must_continue: None,
            status_msg: String::from("Your turn (White) - select a piece"),
            white_count: 12,
            black_count: 12,
            game_over: false,
            bot_thinking: false,
            variant,
            needs_clear: true,
        }
    }

    // returns all moves for a piece; captures take priority via any_capture logic externally
    fn get_moves(&self, pos: Pos) -> Vec<(Pos, Option<Pos>)> {
        let piece = self.board[pos.row][pos.col];
        if piece == Piece::None {
            return vec![];
        }
        let mut moves = Vec::new();

        if self.variant.orthogonal() {
            // turkish: orthogonal (N/E/S/W), no diagonal
            let forward_dirs: Vec<(i32, i32)> = if piece.is_king() {
                vec![(-1, 0), (1, 0), (0, -1), (0, 1)]
            } else if piece.is_white() {
                vec![(-1, 0), (0, -1), (0, 1)]
            } else {
                vec![(1, 0), (0, -1), (0, 1)]
            };
            let capture_dirs: Vec<(i32, i32)> = vec![(-1, 0), (1, 0), (0, -1), (0, 1)];

            if piece.is_king() {
                for (dr, dc) in &forward_dirs {
                    let mut dist = 1i32;
                    loop {
                        let nr = pos.row as i32 + dr * dist;
                        let nc = pos.col as i32 + dc * dist;
                        if nr < 0 || nr >= SIZE as i32 || nc < 0 || nc >= SIZE as i32 { break; }
                        let to = Pos::new(nr as usize, nc as usize);
                        if self.board[to.row][to.col] == Piece::None {
                            moves.push((to, None));
                            dist += 1;
                        } else {
                            let target = self.board[to.row][to.col];
                            let is_enemy = if piece.is_white() { target.is_black() } else { target.is_white() };
                            if is_enemy {
                                let mut land = dist + 1;
                                loop {
                                    let jr = pos.row as i32 + dr * land;
                                    let jc = pos.col as i32 + dc * land;
                                    if jr < 0 || jr >= SIZE as i32 || jc < 0 || jc >= SIZE as i32 { break; }
                                    let jump = Pos::new(jr as usize, jc as usize);
                                    if self.board[jump.row][jump.col] == Piece::None {
                                        moves.push((jump, Some(to)));
                                        land += 1;
                                    } else { break; }
                                }
                            }
                            break;
                        }
                    }
                }
            } else {
                for (dr, dc) in &forward_dirs {
                    let nr = pos.row as i32 + dr;
                    let nc = pos.col as i32 + dc;
                    if nr >= 0 && nr < SIZE as i32 && nc >= 0 && nc < SIZE as i32 {
                        let to = Pos::new(nr as usize, nc as usize);
                        if self.board[to.row][to.col] == Piece::None {
                            moves.push((to, None));
                        }
                    }
                }
                for (dr, dc) in &capture_dirs {
                    let nr = pos.row as i32 + dr;
                    let nc = pos.col as i32 + dc;
                    if nr >= 0 && nr < SIZE as i32 && nc >= 0 && nc < SIZE as i32 {
                        let to = Pos::new(nr as usize, nc as usize);
                        let target = self.board[to.row][to.col];
                        let is_enemy = if piece.is_white() { target.is_black() } else { target.is_white() };
                        if is_enemy {
                            let jr = pos.row as i32 + dr * 2;
                            let jc = pos.col as i32 + dc * 2;
                            if jr >= 0 && jr < SIZE as i32 && jc >= 0 && jc < SIZE as i32 {
                                let jump = Pos::new(jr as usize, jc as usize);
                                if self.board[jump.row][jump.col] == Piece::None {
                                    moves.push((jump, Some(to)));
                                }
                            }
                        }
                    }
                }
            }
            return moves;
        }

        // diagonal variants (russian, english, brazilian)
        // all four diagonals for kings, forward-only for regular pieces
        let move_dirs: Vec<(i32, i32)> = if piece.is_king() {
            vec![(-1, -1), (-1, 1), (1, -1), (1, 1)]
        } else if piece.is_white() {
            vec![(-1, -1), (-1, 1)]
        } else {
            vec![(1, -1), (1, 1)]
        };
        // captures allowed in all four directions for regular pieces too (russian rules)
        let capture_dirs: Vec<(i32, i32)> = if self.variant.backward_capture() || piece.is_king() {
            vec![(-1, -1), (-1, 1), (1, -1), (1, 1)]
        } else {
            move_dirs.clone()
        };

        if piece.is_king() && self.variant.flying_king() {
            // flying king: slides any number of squares, captures by jumping over an enemy
            for (dr, dc) in &move_dirs {
                let mut dist = 1i32;
                loop {
                    let nr = pos.row as i32 + dr * dist;
                    let nc = pos.col as i32 + dc * dist;
                    if nr < 0 || nr >= SIZE as i32 || nc < 0 || nc >= SIZE as i32 {
                        break;
                    }
                    let to = Pos::new(nr as usize, nc as usize);
                    if self.board[to.row][to.col] == Piece::None {
                        moves.push((to, None));
                        dist += 1;
                    } else {
                        // check if it's an enemy we can jump over
                        let target = self.board[to.row][to.col];
                        let is_enemy = if piece.is_white() {
                            target.is_black()
                        } else {
                            target.is_white()
                        };
                        if is_enemy {
                            // land on any empty square beyond the captured piece
                            let mut land = dist + 1;
                            loop {
                                let jr = pos.row as i32 + dr * land;
                                let jc = pos.col as i32 + dc * land;
                                if jr < 0 || jr >= SIZE as i32 || jc < 0 || jc >= SIZE as i32 {
                                    break;
                                }
                                let jump = Pos::new(jr as usize, jc as usize);
                                if self.board[jump.row][jump.col] == Piece::None {
                                    moves.push((jump, Some(to)));
                                    land += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                        break;
                    }
                }
            }
        } else {
            // regular piece or short king: one step forward, captures per variant rules
            for (dr, dc) in &move_dirs {
                let nr = pos.row as i32 + dr;
                let nc = pos.col as i32 + dc;
                if nr >= 0 && nr < SIZE as i32 && nc >= 0 && nc < SIZE as i32 {
                    let to = Pos::new(nr as usize, nc as usize);
                    if self.board[to.row][to.col] == Piece::None {
                        moves.push((to, None));
                    }
                }
            }
            for (dr, dc) in &capture_dirs {
                let nr = pos.row as i32 + dr;
                let nc = pos.col as i32 + dc;
                if nr >= 0 && nr < SIZE as i32 && nc >= 0 && nc < SIZE as i32 {
                    let to = Pos::new(nr as usize, nc as usize);
                    let target = self.board[to.row][to.col];
                    let is_enemy = if piece.is_white() {
                        target.is_black()
                    } else {
                        target.is_white()
                    };
                    if is_enemy {
                        let jr = pos.row as i32 + dr * 2;
                        let jc = pos.col as i32 + dc * 2;
                        if jr >= 0 && jr < SIZE as i32 && jc >= 0 && jc < SIZE as i32 {
                            let jump = Pos::new(jr as usize, jc as usize);
                            if self.board[jump.row][jump.col] == Piece::None {
                                moves.push((jump, Some(to)));
                            }
                        }
                    }
                }
            }
        }
        moves
    }

    fn get_captures(&self, pos: Pos) -> Vec<(Pos, Option<Pos>)> {
        self.get_moves(pos)
            .into_iter()
            .filter(|(_, cap)| cap.is_some())
            .collect()
    }

    // checks if any piece of the given side has a capture available
    fn any_capture_for(&self, white: bool) -> bool {
        for row in 0..SIZE {
            for col in 0..SIZE {
                let piece = self.board[row][col];
                if piece != Piece::None && piece.belongs_to(white) {
                    if !self.get_captures(Pos::new(row, col)).is_empty() {
                        return true;
                    }
                }
            }
        }
        false
    }

    // all valid moves for current player (enforces mandatory capture)
    fn all_valid_moves_for(&self, white: bool) -> Vec<(Pos, Pos, Option<Pos>)> {
        let must_capture = self.any_capture_for(white);
        let mut result = Vec::new();
        for row in 0..SIZE {
            for col in 0..SIZE {
                let piece = self.board[row][col];
                if piece != Piece::None && piece.belongs_to(white) {
                    let pos = Pos::new(row, col);
                    let moves = if must_capture {
                        self.get_captures(pos)
                    } else {
                        self.get_moves(pos)
                    };
                    for (to, cap) in moves {
                        result.push((pos, to, cap));
                    }
                }
            }
        }
        result
    }

    fn select(&mut self, pos: Pos) {
        let piece = self.board[pos.row][pos.col];
        if piece == Piece::None || !piece.belongs_to(self.white_turn) {
            self.status_msg = String::from("Select your own piece!");
            return;
        }

        let must_capture = self.any_capture_for(self.white_turn);
        let moves = if must_capture {
            let caps = self.get_captures(pos);
            if caps.is_empty() {
                self.status_msg = String::from("Must capture! Pick a piece that can.");
                return;
            }
            caps
        } else {
            self.get_moves(pos)
        };

        if moves.is_empty() {
            self.status_msg = String::from("This piece has no moves.");
            return;
        }

        self.selected = Some(pos);
        self.valid_moves = moves;
        self.status_msg = String::from("Move selected piece (arrows + enter)");
    }

    // executes a move given from/to/captured - returns true if turn ends
    fn execute_move(&mut self, from: Pos, dest: Pos, captured: Option<Pos>) -> bool {
        let mut piece = self.board[from.row][from.col];
        self.board[from.row][from.col] = Piece::None;

        if let Some(cap_pos) = captured {
            self.board[cap_pos.row][cap_pos.col] = Piece::None;
            if self.white_turn {
                self.black_count = self.black_count.saturating_sub(1);
            } else {
                self.white_count = self.white_count.saturating_sub(1);
            }
        }

        // promote to king
        if piece == Piece::White && dest.row == 0 {
            piece = Piece::WhiteKing;
        } else if piece == Piece::Black && dest.row == SIZE - 1 {
            piece = Piece::BlackKing;
        }

        self.board[dest.row][dest.col] = piece;

        // check win
        if self.white_count == 0 {
            self.status_msg = String::from("Black (bot) wins! Press R to restart.");
            self.game_over = true;
            self.selected = None;
            self.valid_moves.clear();
            return true;
        }
        if self.black_count == 0 {
            self.status_msg = String::from("You win! Press R to restart.");
            self.game_over = true;
            self.selected = None;
            self.valid_moves.clear();
            return true;
        }

        // check multi-jump
        if captured.is_some() {
            let further = self.get_captures(dest);
            if !further.is_empty() {
                self.selected = Some(dest);
                self.valid_moves = further;
                self.must_continue = Some(dest);
                if self.white_turn {
                    self.status_msg = String::from("Continue jumping!");
                }
                return false; // turn continues
            }
        }

        true // turn ends
    }

    fn end_turn(&mut self) {
        self.must_continue = None;
        self.selected = None;
        self.valid_moves.clear();
        self.white_turn = !self.white_turn;

        if !self.has_any_moves(self.white_turn) {
            let winner = if self.white_turn {
                "Bot (Black)"
            } else {
                "You (White)"
            };
            self.status_msg = format!("No moves left - {} wins! Press R to restart.", winner);
            self.game_over = true;
            return;
        }

        if self.white_turn {
            self.status_msg = String::from("Your turn (White) - select a piece");
        } else {
            self.status_msg = String::from("Bot is thinking...");
            self.bot_thinking = true;
        }
    }

    fn try_move(&mut self, to: Pos) {
        let mv = self.valid_moves.iter().find(|(t, _)| *t == to).cloned();
        if let Some((dest, captured)) = mv {
            let from = self.selected.unwrap();
            let turn_ended = self.execute_move(from, dest, captured);
            if turn_ended && !self.game_over {
                self.end_turn();
            }
        } else {
            self.status_msg = String::from("Invalid move - pick a highlighted square.");
        }
    }

    // simple bot: prefers captures, otherwise random move
    fn bot_move(&mut self) {
        self.bot_thinking = false;
        let moves = self.all_valid_moves_for(false);
        if moves.is_empty() {
            return;
        }

        // score: captures > normal, king moves > pawn moves, advance forward
        let best = moves
            .iter()
            .max_by_key(|(from, to, cap)| {
                let mut score = 0i32;
                if cap.is_some() {
                    score += 1000;
                }
                let piece = self.board[from.row][from.col];
                if piece.is_king() {
                    score += 100;
                }
                // prefer advancing (higher row for black)
                score += to.row as i32 * 10;
                // prefer center columns
                let center_dist = (to.col as i32 - 3).abs().min((to.col as i32 - 4).abs());
                score -= center_dist * 2;
                score
            })
            .cloned()
            .unwrap();

        let (from, dest, captured) = best;
        let turn_ended = self.execute_move(from, dest, captured);
        if turn_ended && !self.game_over {
            self.end_turn();
        } else if !turn_ended && !self.game_over {
            // bot continues multi-jump automatically
            self.bot_continue_jump();
        }
    }

    // handles bot multi-jump chain
    fn bot_continue_jump(&mut self) {
        while self.must_continue.is_some() && !self.game_over {
            let pos = self.must_continue.unwrap();
            let caps = self.get_captures(pos);
            if caps.is_empty() {
                break;
            }
            let (dest, captured) = caps[0];
            let turn_ended = self.execute_move(pos, dest, captured);
            if turn_ended {
                if !self.game_over {
                    self.end_turn();
                }
                break;
            }
        }
    }

    fn has_any_moves(&self, white: bool) -> bool {
        !self.all_valid_moves_for(white).is_empty()
    }

    fn handle_key(&mut self, key: KeyCode) {
        if self.game_over {
            if key == KeyCode::Char('r') || key == KeyCode::Char('R') {
                let v = self.variant;
                *self = Game::new(v);
            }
            return;
        }

        // block input during bot turn
        if !self.white_turn {
            return;
        }

        match key {
            KeyCode::Up => {
                if self.cursor.row > 0 {
                    self.cursor.row -= 1;
                }
            }
            KeyCode::Down => {
                if self.cursor.row < SIZE - 1 {
                    self.cursor.row += 1;
                }
            }
            KeyCode::Left => {
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor.col < SIZE - 1 {
                    self.cursor.col += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let pos = self.cursor;
                if let Some(sel) = self.selected {
                    if pos == sel {
                        if self.must_continue.is_none() {
                            self.selected = None;
                            self.valid_moves.clear();
                            self.status_msg = String::from("Your turn (White) - select a piece");
                        }
                    } else {
                        self.try_move(pos);
                    }
                } else {
                    self.select(pos);
                }
            }
            KeyCode::Esc => {
                if self.must_continue.is_none() {
                    self.selected = None;
                    self.valid_moves.clear();
                    self.status_msg = String::from("Your turn (White) - select a piece");
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                let v = self.variant;
                *self = Game::new(v);
            }
            _ => {}
        }
    }
}

const CELL_W: u16 = 5;
const CELL_H: u16 = 3;

fn draw(stdout: &mut impl Write, game: &mut Game) -> io::Result<()> {
    if game.needs_clear {
        queue!(stdout, terminal::Clear(ClearType::All))?;
        game.needs_clear = false;
    }
    queue!(stdout, cursor::MoveTo(0, 0))?;

    queue!(
        stdout,
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        cursor::MoveTo(2, 0),
        Print("  CHECKERS"),
        SetAttribute(Attribute::Reset),
    )?;

    queue!(
        stdout,
        cursor::MoveTo(20, 0),
        SetForegroundColor(Color::White),
        SetAttribute(Attribute::Bold),
        Print(format!("You (W): {}  ", game.white_count)),
        SetForegroundColor(Color::Rgb {
            r: 30,
            g: 30,
            b: 30
        }),
        SetBackgroundColor(Color::Grey),
        Print(format!("Bot (B): {}", game.black_count)),
        SetAttribute(Attribute::Reset),
        ResetColor,
    )?;

    // column labels
    for col in 0..SIZE {
        let x = 4 + col as u16 * CELL_W + CELL_W / 2;
        queue!(
            stdout,
            cursor::MoveTo(20, 0),
            SetForegroundColor(Color::White),
            SetAttribute(Attribute::Bold),
            Print(format!("You (W): {:>2}  ", game.white_count)),
            SetForegroundColor(Color::Rgb {
                r: 30,
                g: 30,
                b: 30
            }),
            SetBackgroundColor(Color::Grey),
            Print(format!("Bot (B): {:>2}", game.black_count)),
            SetAttribute(Attribute::Reset),
            ResetColor,
        )?;
    }

    let board_start_y: u16 = 2;

    for row in 0..SIZE {
        // row label
        queue!(
            stdout,
            cursor::MoveTo(2, board_start_y + row as u16 * CELL_H + CELL_H / 2),
            SetForegroundColor(Color::DarkGrey),
            Print(8 - row),
        )?;

        for col in 0..SIZE {
            let x = 4 + col as u16 * CELL_W;
            let y = board_start_y + row as u16 * CELL_H;
            let pos = Pos::new(row, col);

            let is_dark = (row + col) % 2 == 1;
            let is_cursor = game.white_turn && game.cursor == pos;
            let is_selected = game.selected == Some(pos);
            let is_valid_move = game.valid_moves.iter().any(|(t, _)| *t == pos);

            let bg = if is_cursor {
                Color::Rgb {
                    r: 60,
                    g: 60,
                    b: 10,
                }
            } else if is_selected {
                Color::Green
            } else if is_valid_move {
                Color::Rgb {
                    r: 0,
                    g: 110,
                    b: 45,
                }
            } else if is_dark {
                Color::Rgb {
                    r: 50,
                    g: 50,
                    b: 70,
                }
            } else {
                Color::Rgb {
                    r: 200,
                    g: 185,
                    b: 155,
                }
            };

            for dy in 0..CELL_H {
                queue!(
                    stdout,
                    cursor::MoveTo(x, y + dy),
                    SetBackgroundColor(bg),
                    Print(" ".repeat(CELL_W as usize)),
                )?;
            }

            let piece = game.board[row][col];
            if piece != Piece::None {
                // white = bright white, black = dark grey/charcoal on board
                let (symbol, fg) = match piece {
                    Piece::White => ("( )", Color::White),
                    Piece::WhiteKing => ("(K)", Color::White),
                    Piece::Black => (
                        "( )",
                        Color::Rgb {
                            r: 80,
                            g: 80,
                            b: 100,
                        },
                    ),
                    Piece::BlackKing => (
                        "(K)",
                        Color::Rgb {
                            r: 80,
                            g: 80,
                            b: 100,
                        },
                    ),
                    Piece::None => unreachable!(),
                };
                queue!(
                    stdout,
                    cursor::MoveTo(x + 1, y + CELL_H / 2),
                    SetBackgroundColor(bg),
                    SetForegroundColor(fg),
                    SetAttribute(Attribute::Bold),
                    Print(symbol),
                    SetAttribute(Attribute::Reset),
                )?;
            }

            if is_valid_move && piece == Piece::None {
                queue!(
                    stdout,
                    cursor::MoveTo(x + CELL_W / 2, y + CELL_H / 2),
                    SetBackgroundColor(bg),
                    SetForegroundColor(Color::Rgb {
                        r: 100,
                        g: 255,
                        b: 100
                    }),
                    Print("•"),
                    SetAttribute(Attribute::Reset),
                )?;
            }
        }

        queue!(stdout, ResetColor)?;
    }

    let status_y = board_start_y + SIZE as u16 * CELL_H + 1;
    let status_color = if game.game_over {
        Color::Yellow
    } else if game.bot_thinking || !game.white_turn {
        Color::Rgb {
            r: 180,
            g: 180,
            b: 180,
        }
    } else {
        Color::Cyan
    };
    let padded_msg = format!("{:<70}", &game.status_msg);
    queue!(
        stdout,
        cursor::MoveTo(2, status_y),
        terminal::Clear(ClearType::UntilNewLine),
        SetForegroundColor(status_color),
        Print(&padded_msg),
        ResetColor,
    )?;

    let legend_y = status_y + 1;
    queue!(
        stdout,
        cursor::MoveTo(2, legend_y),
        SetForegroundColor(Color::DarkGrey),
        Print("Arrows: cursor  Enter/Space: select/move  Esc: deselect  R: restart  M: menu  Q: quit"),
        ResetColor,
    )?;

    // sidebar
    let turn_x = 4 + SIZE as u16 * CELL_W + 3;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 1),
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        Print("TURN"),
        SetAttribute(Attribute::Reset),
    )?;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 3),
        SetForegroundColor(if game.white_turn {
            Color::White
        } else {
            Color::DarkGrey
        }),
        SetAttribute(if game.white_turn {
            Attribute::Bold
        } else {
            Attribute::Dim
        }),
        Print("▶ You (White)"),
        SetAttribute(Attribute::Reset),
    )?;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 4),
        SetForegroundColor(if !game.white_turn {
            Color::Grey
        } else {
            Color::DarkGrey
        }),
        SetAttribute(if !game.white_turn {
            Attribute::Bold
        } else {
            Attribute::Dim
        }),
        Print("▶ Bot (Black)"),
        SetAttribute(Attribute::Reset),
    )?;

    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 7),
        SetForegroundColor(Color::DarkGrey),
        Print("Pieces:"),
    )?;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 8),
        SetForegroundColor(Color::White),
        Print("( ) normal"),
    )?;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 9),
        SetForegroundColor(Color::White),
        Print("(K) king"),
        ResetColor,
    )?;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 12),
        SetForegroundColor(Color::DarkGrey),
        Print("Variant:"),
    )?;
    queue!(
        stdout,
        cursor::MoveTo(turn_x, board_start_y + 13),
        SetForegroundColor(Color::Cyan),
        Print(game.variant.label()),
        ResetColor,
    )?;

    stdout.flush()?;
    Ok(())
}

fn draw_menu(stdout: &mut impl Write, selected: usize, prev_selected: Option<usize>) -> io::Result<()> {
    let variants = [
        Variant::Russian,
        Variant::English,
        Variant::Brazilian,
        Variant::Turkish,
    ];

    // full clear only on first draw
    if prev_selected.is_none() {
        queue!(stdout, terminal::Clear(ClearType::All))?;

        queue!(
            stdout,
            cursor::MoveTo(2, 0),
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            Print("  CHECKERS"),
            SetAttribute(Attribute::Reset),
        )?;

        queue!(
            stdout,
            cursor::MoveTo(4, 2),
            SetForegroundColor(Color::DarkGrey),
            Print("Select a variant:"),
            ResetColor,
        )?;

        queue!(
            stdout,
            cursor::MoveTo(4, 22),
            SetForegroundColor(Color::DarkGrey),
            Print("Up/Down: navigate    Enter/Space: confirm    Q: quit"),
            ResetColor,
        )?;
    }

    // redraw only the rows that changed (or all on first draw)
    let redraw: Vec<usize> = if let Some(prev) = prev_selected {
        vec![prev, selected]
    } else {
        (0..variants.len()).collect()
    };

    for &i in &redraw {
        let variant = &variants[i];
        let row = 4 + i as u16 * 4;
        let is_sel = i == selected;

        let (bg, fg_label, fg_desc) = if is_sel {
            (
                Color::Rgb { r: 30, g: 60, b: 90 },
                Color::White,
                Color::Rgb { r: 180, g: 220, b: 255 },
            )
        } else {
            (
                Color::Rgb { r: 20, g: 20, b: 30 },
                Color::DarkGrey,
                Color::Rgb { r: 80, g: 80, b: 100 },
            )
        };

        let prefix = if is_sel { "▶ " } else { "  " };

        queue!(
            stdout,
            cursor::MoveTo(4, row),
            SetBackgroundColor(bg),
            Print(" ".repeat(44)),
            cursor::MoveTo(4, row + 1),
            Print(" ".repeat(44)),
            cursor::MoveTo(4, row + 2),
            Print(" ".repeat(44)),
            cursor::MoveTo(4, row + 3),
            Print(" ".repeat(44)),
            cursor::MoveTo(5, row + 1),
            SetForegroundColor(fg_label),
            SetAttribute(if is_sel { Attribute::Bold } else { Attribute::Dim }),
            Print(format!("{}{}", prefix, variant.label())),
            SetAttribute(Attribute::Reset),
            SetBackgroundColor(bg),
            cursor::MoveTo(7, row + 2),
            SetForegroundColor(fg_desc),
            Print(variant.description()),
            SetAttribute(Attribute::Reset),
            ResetColor,
        )?;
    }

    stdout.flush()?;
    Ok(())
}

fn run_menu(stdout: &mut impl Write) -> io::Result<Option<Variant>> {
    let variants = [
        Variant::Russian,
        Variant::English,
        Variant::Brazilian,
        Variant::Turkish,
    ];
    let mut selected = 0usize;
    let mut prev_selected: Option<usize> = None;

    loop {
        draw_menu(stdout, selected, prev_selected)?;
        prev_selected = Some(selected);

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, kind: KeyEventKind::Press, .. }) = event::read()? {
                match code {
                    KeyCode::Up => {
                        if selected > 0 { selected -= 1; }
                    }
                    KeyCode::Down => {
                        if selected < variants.len() - 1 { selected += 1; }
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        return Ok(Some(variants[selected]));
                    }
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
    execute!(stdout, terminal::Clear(ClearType::All))?;

    let variant = match run_menu(&mut stdout)? {
        Some(v) => v,
        None => {
            execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
            terminal::disable_raw_mode()?;
            return Ok(());
        }
    };

    let mut game = Game::new(variant);

    loop {
        draw(&mut stdout, &mut game)?;
        stdout.flush()?;

        if !game.white_turn && !game.game_over {
            std::thread::sleep(Duration::from_millis(400));
            game.bot_move();
            continue;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('m') | KeyCode::Char('M') => {
                        execute!(&mut stdout, terminal::Clear(ClearType::All))?;
                        let new_variant = match run_menu(&mut stdout)? {
                            Some(v) => v,
                            None => break,
                        };
                        game = Game::new(new_variant);
                    }
                    other => game.handle_key(other),
                }
            }
        }
    }

    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}
